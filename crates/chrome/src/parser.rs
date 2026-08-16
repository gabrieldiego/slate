/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#[cfg(not(any(target_os = "android", target_env = "ohos")))]
use std::path::{Path, PathBuf};

use servo::{ServoUrl, is_reg_domain};
use slate_broadwebd::{TOR_HTTP_SCHEME, TOR_HTTPS_SCHEME, is_onion_host, tor_url_from_http_url};

#[cfg(not(any(target_os = "android", target_env = "ohos")))]
pub fn parse_url_or_filename(cwd: &Path, input: &str) -> Result<ServoUrl, ()> {
    match ServoUrl::parse(input) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            url::Url::from_file_path(&*cwd.join(input)).map(ServoUrl::from_url)
        }
        Err(_) => Err(()),
    }
}

#[cfg(not(any(target_os = "android", target_env = "ohos")))]
pub fn get_default_url(
    url_opt: Option<&str>,
    cwd: impl AsRef<Path>,
    exists: impl FnOnce(&PathBuf) -> bool,
    preferences: &crate::prefs::ServoShellPreferences,
) -> ServoUrl {
    // If the url is not provided, we fallback to the homepage in prefs,
    // or a blank page in case the homepage is not set either.
    let mut new_url = None;
    let cmdline_url = url_opt.map(|s| s.to_string()).and_then(|url_string| {
        parse_url_or_filename(cwd.as_ref(), &url_string)
            .inspect_err(|&error| {
                log::warn!("URL parsing failed ({:?}).", error);
            })
            .ok()
    });

    if let Some(url) = cmdline_url.clone() {
        // Check if the URL path corresponds to a file
        match (url.scheme(), url.host(), url.to_file_path()) {
            ("file", None, Ok(ref path)) if exists(path) => {
                new_url = cmdline_url;
            }
            (scheme, None, Err(_)) if is_localhost(scheme) || is_domain_like(scheme) => {
                new_url = ServoUrl::parse(&format!("http://{}:{}", scheme, url.path())).ok();
            }
            _ => {}
        }
    }

    #[allow(
        clippy::collapsible_if,
        reason = "let chains are not available in 1.85"
    )]
    if new_url.is_none() {
        if let Some(url_opt) = url_opt {
            new_url = location_bar_input_to_url(url_opt, &preferences.searchpage);
        }
    }

    let pref_url = parse_url_or_filename(cwd.as_ref(), &preferences.homepage).ok();
    let blank_url = ServoUrl::parse("about:blank").ok();

    new_url.or(pref_url).or(blank_url).unwrap()
}

/// Interpret an input URL.
///
/// If this is not a valid URL, try to "fix" it by adding a scheme or if all else fails,
/// interpret the string as a search term.
pub(crate) fn location_bar_input_to_url(request: &str, searchpage: &str) -> Option<ServoUrl> {
    let request = request.trim();
    let input_url = ServoUrl::parse(request).ok();
    if let Some(url) = input_url {
        match (url.scheme(), url.host(), url.to_file_path()) {
            (scheme, None, Err(_)) if is_localhost(scheme) || is_domain_like(scheme) => {
                ServoUrl::parse(&format!("http://{}:{}", scheme, url.path())).ok()
            }
            ("http" | "https", _, _) => match tor_url_from_http_url(url.as_url()) {
                Ok(Some(address)) => ServoUrl::parse(&address).ok(),
                Ok(None) => Some(url),
                Err(_) => try_as_search_page(request, searchpage),
            },
            ("tor+http" | "tor+https", _, _) => Some(url),
            _ => Some(url),
        }
    } else {
        try_as_ipfs_address(request)
            .or_else(|| try_as_onion_address(request))
            .or_else(|| try_as_file(request))
            .or_else(|| try_as_domain(request))
            .or_else(|| try_as_search_page(request, searchpage))
    }
}

fn try_as_ipfs_address(request: &str) -> Option<ServoUrl> {
    normalize_ipfs_path_address(request)
        .or_else(|| normalize_bare_ipfs_cid(request))
        .and_then(|address| ServoUrl::parse(&address).ok())
}

fn normalize_ipfs_path_address(input: &str) -> Option<String> {
    let path = input.strip_prefix('/').unwrap_or(input);
    let lower = path.to_ascii_lowercase();
    let (scheme, rest) = if lower.starts_with("ipfs/") {
        ("ipfs", &path["ipfs/".len()..])
    } else if lower.starts_with("ipns/") {
        ("ipns", &path["ipns/".len()..])
    } else {
        return None;
    };

    if rest.is_empty() || rest.chars().any(char::is_whitespace) {
        return None;
    }

    let name_end = rest
        .find(|ch| matches!(ch, '/' | '?' | '#'))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }

    Some(format!("{scheme}://{rest}"))
}

fn normalize_bare_ipfs_cid(input: &str) -> Option<String> {
    let (name, rest) = split_address_name(input);
    if rest.is_some_and(|suffix| suffix.chars().any(char::is_whitespace)) {
        return None;
    }

    if is_cidv0_like(name) {
        return Some(format!("ipfs://{input}"));
    }

    let lower = name.to_ascii_lowercase();
    if is_cidv1_base32_like(&lower) && name.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        let suffix = rest.unwrap_or_default();
        return Some(format!("ipfs://{lower}{suffix}"));
    }

    None
}

fn split_address_name(input: &str) -> (&str, Option<&str>) {
    let name_end = input
        .find(|ch| matches!(ch, '/' | '?' | '#'))
        .unwrap_or(input.len());
    let name = &input[..name_end];
    let rest = (name_end < input.len()).then_some(&input[name_end..]);
    (name, rest)
}

fn try_as_onion_address(request: &str) -> Option<ServoUrl> {
    if request.chars().any(char::is_whitespace) {
        return None;
    }
    let (name, rest) = split_address_name(request);
    if !is_onion_host(name) {
        return None;
    }

    let mut address = format!(
        "{TOR_HTTP_SCHEME}://{}",
        name.trim_end_matches('.').to_ascii_lowercase()
    );
    match rest {
        Some(rest) if rest.starts_with('?') => {
            address.push('/');
            address.push_str(rest);
        }
        Some(rest) => address.push_str(rest),
        None => address.push('/'),
    }
    ServoUrl::parse(&address).ok()
}

fn is_cidv0_like(input: &str) -> bool {
    input.len() == 46
        && input.starts_with("Qm")
        && input.bytes().all(|byte| {
            b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&byte)
        })
}

fn is_cidv1_base32_like(input: &str) -> bool {
    input.len() >= 32
        && matches!(input.get(..4), Some("bafy" | "bafk"))
        && input
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'2'..=b'7'))
}

fn try_as_file(request: &str) -> Option<ServoUrl> {
    if request.starts_with('/') {
        return ServoUrl::parse(&format!("file://{}", request)).ok();
    }
    None
}

fn try_as_domain(request: &str) -> Option<ServoUrl> {
    if !request.contains(' ') && is_reg_domain(request) || is_domain_like(request) {
        return ServoUrl::parse(&format!("https://{}", request)).ok();
    }
    None
}

fn try_as_search_page(request: &str, searchpage: &str) -> Option<ServoUrl> {
    if request.is_empty() {
        return None;
    }
    ServoUrl::parse(&searchpage.replace("%s", request)).ok()
}

fn is_domain_like(s: &str) -> bool {
    !s.starts_with('/') && s.contains('/')
        || (!s.contains(' ') && !s.starts_with('.') && s.split('.').count() > 1)
}

fn is_localhost(s: &str) -> bool {
    s == "localhost"
}
