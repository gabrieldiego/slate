/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Slate-owned internal browser pages.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use headers::{ContentType, HeaderMapExt};
use log::warn;
use servo::ServoUrl;
use servo::protocol_handler::{
    DoneChannel, FetchContext, HttpStatus, NetworkError, ProtocolHandler, Request,
    ResourceFetchTiming, Response, ResponseBody,
};
use slate_broadwebd::{
    FetchDisposition, HttpFetchRequest, StateRoot, TemporaryDownloadRecord,
    default_session_state_root,
};
use slate_storage::{DEFAULT_PROFILE_ID, SlateProfileDatabase};
use url::Url;

use crate::desktop::key_bindings::{
    SlateKeyBindings, current_key_bindings_json_value, initialize_key_bindings_from_database,
    key_bindings_from_settings_url, persist_key_bindings_to_database, set_current_key_bindings,
};
use crate::desktop::protocols::broadweb::{
    broadweb_download_ready_html, escape_html_text, fetch_with_default_broadwebd,
};
use crate::desktop::protocols::resource::ResourceProtocolHandler;

pub(crate) const CHROME_ELEMENT_ZOOM_SETTING_DEFAULT: f32 = 0.9;
pub(crate) const CHROME_ELEMENT_ZOOM_SETTING_MIN: f32 = 0.75;
pub(crate) const CHROME_ELEMENT_ZOOM_SETTING_MAX: f32 = 1.15;

const CHROME_ELEMENT_ZOOM_PERCENT_DEFAULT: u32 = 90;
const CHROME_ELEMENT_ZOOM_PERCENT_MIN: u32 = 75;
const CHROME_ELEMENT_ZOOM_PERCENT_MAX: u32 = 115;
const CHROME_ELEMENT_ZOOM_SETTING_KEY: &str = "chrome.zoom";

static CHROME_ELEMENT_ZOOM_PERCENT: AtomicU32 = AtomicU32::new(CHROME_ELEMENT_ZOOM_PERCENT_DEFAULT);

#[derive(Default)]
pub struct SlateProtocolHandler {
    database: Option<SlateProfileDatabase>,
}

impl SlateProtocolHandler {
    pub(crate) fn new(database: SlateProfileDatabase) -> Self {
        initialize_chrome_settings_from_database(&database);
        Self {
            database: Some(database),
        }
    }
}

impl ProtocolHandler for SlateProtocolHandler {
    fn privileged_paths(&self) -> &'static [&'static str] {
        &[
            "blank",
            "home",
            "web",
            "calendar",
            "chat",
            "messages",
            "contacts",
            "files",
            "settings",
            "settings/state",
            "settings/preview",
            "settings/save",
            "settings/apply",
            "downloads",
            "downloads/state",
            "download",
        ]
    }

    fn is_fetchable(&self) -> bool {
        true
    }

    fn load(
        &self,
        request: &mut Request,
        done_chan: &mut DoneChannel,
        context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        if is_slate_settings_state_url(url.as_url()) {
            return settings_json_response(request, current_chrome_element_zoom_setting());
        }

        if is_slate_downloads_state_url(url.as_url()) {
            return downloads_json_response(request);
        }

        if is_slate_download_request_url(url.as_url()) {
            return download_url_response(request);
        }

        if is_slate_settings_preview_url(url.as_url()) {
            let zoom = chrome_element_zoom_setting_from_url(url.as_url())
                .map(set_current_chrome_element_zoom_setting)
                .unwrap_or_else(current_chrome_element_zoom_setting);
            return settings_json_response(request, zoom);
        }

        if is_slate_settings_save_url(url.as_url()) || is_slate_settings_apply_url(url.as_url()) {
            let zoom = chrome_element_zoom_setting_from_url(url.as_url())
                .map(set_current_chrome_element_zoom_setting)
                .unwrap_or_else(current_chrome_element_zoom_setting);
            self.persist_chrome_element_zoom_setting(zoom);
            if let Some(key_bindings) = key_bindings_from_settings_url(url.as_url()) {
                set_current_key_bindings(key_bindings.clone());
                self.persist_key_bindings(&key_bindings);
            }
            return settings_json_response(request, zoom);
        }

        let resource_path = if is_slate_blank_url(url.as_url()) {
            Some("/slate-blank.html")
        } else if is_slate_home_url(url.as_url()) {
            Some("/slate-home.html")
        } else if is_slate_web_url(url.as_url()) {
            Some("/slate-web.html")
        } else if is_slate_calendar_url(url.as_url()) {
            Some("/slate-calendar.html")
        } else if is_slate_chat_url(url.as_url()) {
            Some("/slate-chat.html")
        } else if is_slate_contacts_url(url.as_url()) {
            Some("/slate-contacts.html")
        } else if is_slate_files_url(url.as_url()) {
            Some("/slate-files.html")
        } else if is_slate_downloads_url(url.as_url()) {
            Some("/slate-downloads.html")
        } else if is_slate_settings_url(url.as_url()) {
            Some("/slate-settings.html")
        } else {
            None
        };

        if let Some(resource_path) = resource_path {
            return ResourceProtocolHandler::response_for_path(
                request,
                done_chan,
                context,
                resource_path,
            );
        }

        Box::pin(std::future::ready(Response::network_error(
            NetworkError::ResourceLoadError("Invalid Slate internal page".to_owned()),
        )))
    }
}

impl SlateProtocolHandler {
    fn persist_chrome_element_zoom_setting(&self, zoom: f32) {
        if let Some(database) = &self.database {
            if let Err(error) = database.set_setting_f32(CHROME_ELEMENT_ZOOM_SETTING_KEY, zoom) {
                warn!("failed to persist chrome zoom setting: {error}");
            }
        }
    }

    fn persist_key_bindings(&self, key_bindings: &SlateKeyBindings) {
        if let Some(database) = &self.database {
            persist_key_bindings_to_database(database, key_bindings);
        }
    }
}

pub(crate) fn initialize_chrome_settings_from_database(database: &SlateProfileDatabase) {
    initialize_key_bindings_from_database(database);
    let zoom = match database.ensure_setting_f32(
        CHROME_ELEMENT_ZOOM_SETTING_KEY,
        CHROME_ELEMENT_ZOOM_SETTING_DEFAULT,
    ) {
        Ok(zoom) => zoom,
        Err(error) => {
            warn!("failed to load chrome zoom setting: {error}");
            CHROME_ELEMENT_ZOOM_SETTING_DEFAULT
        }
    };
    set_current_chrome_element_zoom_setting(zoom);
}

pub(crate) fn clamp_chrome_element_zoom_setting(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(
            CHROME_ELEMENT_ZOOM_SETTING_MIN,
            CHROME_ELEMENT_ZOOM_SETTING_MAX,
        )
    } else {
        CHROME_ELEMENT_ZOOM_SETTING_DEFAULT
    }
}

pub(crate) fn current_chrome_element_zoom_setting() -> f32 {
    chrome_element_zoom_from_percent(CHROME_ELEMENT_ZOOM_PERCENT.load(Ordering::Relaxed))
}

pub(crate) fn set_current_chrome_element_zoom_setting(zoom: f32) -> f32 {
    let percent = chrome_element_zoom_percent(zoom);
    CHROME_ELEMENT_ZOOM_PERCENT.store(percent, Ordering::Relaxed);
    chrome_element_zoom_from_percent(percent)
}

pub(crate) fn chrome_element_zoom_setting_from_url(url: &Url) -> Option<f32> {
    if !(is_slate_settings_url(url) || is_slate_settings_apply_url(url)) {
        return None;
    }

    url.query_pairs()
        .find(|(name, _)| name == "chrome_zoom")
        .and_then(|(_, value)| value.parse::<f32>().ok())
        .map(clamp_chrome_element_zoom_setting)
}

fn chrome_element_zoom_percent(zoom: f32) -> u32 {
    ((clamp_chrome_element_zoom_setting(zoom) * 100.0).round() as u32).clamp(
        CHROME_ELEMENT_ZOOM_PERCENT_MIN,
        CHROME_ELEMENT_ZOOM_PERCENT_MAX,
    )
}

fn chrome_element_zoom_from_percent(percent: u32) -> f32 {
    percent.clamp(
        CHROME_ELEMENT_ZOOM_PERCENT_MIN,
        CHROME_ELEMENT_ZOOM_PERCENT_MAX,
    ) as f32
        / 100.0
}

fn settings_state_json(zoom: f32) -> String {
    let rounded_zoom = ((zoom as f64) * 100.0).round() / 100.0;
    serde_json::json!({
        "chrome_zoom": rounded_zoom,
        "key_bindings": current_key_bindings_json_value(),
    })
    .to_string()
}

fn settings_json_response(
    request: &Request,
    zoom: f32,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(settings_state_json(zoom).into());
    Box::pin(std::future::ready(response))
}

fn downloads_json_response(request: &Request) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(current_downloads_json().into());
    Box::pin(std::future::ready(response))
}

fn download_url_response(request: &Request) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    let request_url = request.current_url();
    let timing = ResourceFetchTiming::new(request.timing_type());
    let response = match download_request_from_url(request_url.as_url()) {
        Ok(fetch_request) => match fetch_with_default_broadwebd(fetch_request) {
            Ok(fetch_response) => download_fetch_response(request_url, timing, fetch_response),
            Err(error) => slate_download_error_response(
                request_url,
                timing,
                "Download Failed",
                &error.to_string(),
                502,
            ),
        },
        Err(error) => slate_download_error_response(
            request_url,
            timing,
            "Invalid Download Request",
            &error,
            400,
        ),
    };
    Box::pin(std::future::ready(response))
}

fn download_fetch_response(
    request_url: ServoUrl,
    timing: ResourceFetchTiming,
    fetch_response: slate_broadwebd::HttpFetchResponse,
) -> Response {
    let mut response = Response::new(request_url, timing);
    response.headers.typed_insert(ContentType::html());

    let (status_code, body) = if matches!(
        &fetch_response.disposition,
        FetchDisposition::Download { .. }
    ) && fetch_response.download.is_some()
    {
        (200, broadweb_download_ready_html(&fetch_response))
    } else if let FetchDisposition::ErrorPage { status_code } = &fetch_response.disposition {
        (
            *status_code,
            slate_download_error_html(
                "Download Failed",
                &format!("HTTP status {status_code} for {}", fetch_response.final_url),
            ),
        )
    } else {
        (
            502,
            slate_download_error_html(
                "Download Not Saved",
                "Slate fetched the requested URL, but broadwebd did not classify it as a download.",
            ),
        )
    };

    response.status = slate_http_status(status_code);
    *response.body.lock() = ResponseBody::Done(body.into_bytes());
    response
}

fn slate_download_error_response(
    request_url: ServoUrl,
    timing: ResourceFetchTiming,
    title: &str,
    message: &str,
    status_code: u16,
) -> Response {
    let mut response = Response::new(request_url, timing);
    response.status = slate_http_status(status_code);
    response.headers.typed_insert(ContentType::html());
    *response.body.lock() = ResponseBody::Done(slate_download_error_html(title, message).into());
    response
}

fn slate_http_status(status_code: u16) -> HttpStatus {
    if (100..=599).contains(&status_code) {
        HttpStatus::new_raw(status_code, Vec::new())
    } else {
        HttpStatus::new_error()
    }
}

fn slate_download_error_html(title: &str, message: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>{}</title></head>\
         <body><h1>{}</h1><pre>{}</pre>\
         <p><a href=\"slate://downloads\">Open Downloads</a></p></body></html>",
        escape_html_text(title),
        escape_html_text(title),
        escape_html_text(message)
    )
}

fn current_downloads_json() -> String {
    match current_downloads() {
        Ok(downloads) => downloads_json(&downloads),
        Err(error) => serde_json::json!({
            "downloads": [],
            "error": error.to_string(),
        })
        .to_string(),
    }
}

fn current_downloads() -> Result<Vec<TemporaryDownloadRecord>, slate_broadwebd::BroadwebdError> {
    StateRoot::prepare(default_session_state_root())?.downloads(DEFAULT_PROFILE_ID)
}

fn downloads_json(downloads: &[TemporaryDownloadRecord]) -> String {
    serde_json::json!({
        "downloads": downloads
            .iter()
            .map(download_json)
            .collect::<Vec<serde_json::Value>>()
    })
    .to_string()
}

fn download_json(download: &TemporaryDownloadRecord) -> serde_json::Value {
    serde_json::json!({
        "profile": download.profile,
        "filename": download.filename,
        "path": download.path.to_string_lossy(),
        "file_url": Url::from_file_path(&download.path).ok().map(|url| url.to_string()),
        "size_bytes": download.size_bytes,
    })
}

fn download_request_from_url(url: &Url) -> Result<HttpFetchRequest, String> {
    if !is_slate_download_request_url(url) {
        return Err("not a Slate download request".to_string());
    }

    let target = url
        .query_pairs()
        .find(|(name, _)| name == "url")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| "missing url query parameter".to_string())?;
    let target_url =
        Url::parse(&target).map_err(|error| format!("invalid download URL: {error}"))?;
    if !is_supported_download_target(&target_url) {
        return Err(format!(
            "unsupported download scheme: {}",
            target_url.scheme()
        ));
    }

    let filename = url
        .query_pairs()
        .find(|(name, _)| name == "filename")
        .and_then(|(_, value)| non_empty_download_filename(&value))
        .unwrap_or_else(|| suggested_download_filename(&target_url));

    Ok(HttpFetchRequest::default_profile(target).download_as(filename))
}

fn non_empty_download_filename(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn suggested_download_filename(url: &Url) -> String {
    url.path_segments()
        .and_then(|segments| {
            segments
                .rev()
                .find(|segment| !segment.is_empty())
                .map(str::to_string)
        })
        .or_else(|| url.host_str().map(str::to_string))
        .filter(|filename| !filename.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

fn is_supported_download_target(url: &Url) -> bool {
    matches!(
        url.scheme(),
        "http" | "https" | "ipfs" | "ipns" | "tor+http" | "tor+https"
    )
}

pub(crate) fn is_slate_blank_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("blank") || url.path().trim_start_matches('/') == "blank")
}

pub(crate) fn is_slate_home_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("home") || url.path().trim_start_matches('/') == "home")
}

pub(crate) fn is_slate_web_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("web") || url.path().trim_start_matches('/') == "web")
}

pub(crate) fn is_slate_downloads_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("downloads")
            || url.path().trim_start_matches('/') == "downloads")
}

pub(crate) fn is_slate_calendar_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("calendar") || url.path().trim_start_matches('/') == "calendar")
}

pub(crate) fn is_slate_chat_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (matches!(url.host_str(), Some("chat" | "messages"))
            || matches!(url.path().trim_start_matches('/'), "chat" | "messages"))
}

pub(crate) fn is_slate_contacts_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("contacts") || url.path().trim_start_matches('/') == "contacts")
}

pub(crate) fn is_slate_files_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("files") || url.path().trim_start_matches('/') == "files")
}

pub(crate) fn is_slate_settings_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("settings") || url.path().trim_start_matches('/') == "settings")
}

fn is_slate_downloads_state_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("downloads")
        && url.path().trim_start_matches('/') == "state"
}

fn is_slate_download_request_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("download") || url.path().trim_start_matches('/') == "download")
}

fn is_slate_settings_state_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "state"
}

fn is_slate_settings_preview_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "preview"
}

fn is_slate_settings_save_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "save"
}

fn is_slate_settings_apply_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "apply"
}

#[cfg(test)]
mod tests {
    use super::{
        CHROME_ELEMENT_ZOOM_SETTING_MAX, CHROME_ELEMENT_ZOOM_SETTING_MIN,
        chrome_element_zoom_setting_from_url, download_request_from_url, is_slate_blank_url,
        is_slate_calendar_url, is_slate_chat_url, is_slate_contacts_url,
        is_slate_download_request_url, is_slate_downloads_state_url, is_slate_downloads_url,
        is_slate_files_url, is_slate_home_url, is_slate_settings_apply_url,
        is_slate_settings_preview_url, is_slate_settings_save_url, is_slate_settings_url,
        is_slate_web_url, slate_download_error_html,
    };
    use slate_broadwebd::FetchPurpose;
    use slate_broadwebd::TemporaryDownloadRecord;
    use std::path::PathBuf;
    use url::Url;

    #[test]
    fn slate_home_url_matches_host_and_path_forms() {
        assert!(is_slate_home_url(&Url::parse("slate://home").unwrap()));
        assert!(is_slate_home_url(&Url::parse("slate:home").unwrap()));
        assert!(!is_slate_home_url(&Url::parse("slate://settings").unwrap()));
        assert!(!is_slate_home_url(&Url::parse("https://home").unwrap()));
    }

    #[test]
    fn slate_blank_url_matches_host_and_path_forms() {
        assert!(is_slate_blank_url(&Url::parse("slate://blank").unwrap()));
        assert!(is_slate_blank_url(&Url::parse("slate:blank").unwrap()));
        assert!(!is_slate_blank_url(&Url::parse("slate://home").unwrap()));
        assert!(!is_slate_blank_url(&Url::parse("https://blank").unwrap()));
    }

    #[test]
    fn slate_web_url_matches_host_and_path_forms() {
        assert!(is_slate_web_url(&Url::parse("slate://web").unwrap()));
        assert!(is_slate_web_url(&Url::parse("slate:web").unwrap()));
        assert!(!is_slate_web_url(&Url::parse("slate://home").unwrap()));
        assert!(!is_slate_web_url(&Url::parse("https://web").unwrap()));
    }

    #[test]
    fn slate_downloads_url_matches_host_and_path_forms() {
        assert!(is_slate_downloads_url(
            &Url::parse("slate://downloads").unwrap()
        ));
        assert!(is_slate_downloads_url(
            &Url::parse("slate:downloads").unwrap()
        ));
        assert!(!is_slate_downloads_url(
            &Url::parse("slate://settings").unwrap()
        ));
        assert!(is_slate_downloads_state_url(
            &Url::parse("slate://downloads/state").unwrap()
        ));
    }

    #[test]
    fn slate_calendar_url_matches_host_and_path_forms() {
        assert!(is_slate_calendar_url(
            &Url::parse("slate://calendar").unwrap()
        ));
        assert!(is_slate_calendar_url(
            &Url::parse("slate:calendar").unwrap()
        ));
        assert!(!is_slate_calendar_url(
            &Url::parse("slate://downloads").unwrap()
        ));
        assert!(!is_slate_calendar_url(
            &Url::parse("https://calendar").unwrap()
        ));
    }

    #[test]
    fn slate_chat_url_matches_primary_and_messages_aliases() {
        assert!(is_slate_chat_url(&Url::parse("slate://chat").unwrap()));
        assert!(is_slate_chat_url(&Url::parse("slate:chat").unwrap()));
        assert!(is_slate_chat_url(&Url::parse("slate://messages").unwrap()));
        assert!(is_slate_chat_url(&Url::parse("slate:messages").unwrap()));
        assert!(!is_slate_chat_url(&Url::parse("slate://calendar").unwrap()));
        assert!(!is_slate_chat_url(&Url::parse("https://chat").unwrap()));
    }

    #[test]
    fn slate_contacts_and_files_urls_match_host_and_path_forms() {
        assert!(is_slate_contacts_url(
            &Url::parse("slate://contacts").unwrap()
        ));
        assert!(is_slate_contacts_url(
            &Url::parse("slate:contacts").unwrap()
        ));
        assert!(!is_slate_contacts_url(&Url::parse("slate://chat").unwrap()));
        assert!(is_slate_files_url(&Url::parse("slate://files").unwrap()));
        assert!(is_slate_files_url(&Url::parse("slate:files").unwrap()));
        assert!(!is_slate_files_url(&Url::parse("https://files").unwrap()));
    }

    #[test]
    fn slate_download_request_url_matches_host_and_path_forms() {
        assert!(is_slate_download_request_url(
            &Url::parse("slate://download?url=https%3A%2F%2Fexample.com%2Ffile.zip").unwrap()
        ));
        assert!(is_slate_download_request_url(
            &Url::parse("slate:download?url=https%3A%2F%2Fexample.com%2Ffile.zip").unwrap()
        ));
        assert!(!is_slate_download_request_url(
            &Url::parse("slate://downloads").unwrap()
        ));
        assert!(!is_slate_download_request_url(
            &Url::parse("https://example.com/file.zip").unwrap()
        ));
    }

    #[test]
    fn slate_download_request_builds_broadweb_request() {
        let request = download_request_from_url(
            &Url::parse(
                "slate://download?url=https%3A%2F%2Fexample.com%2Freleases%2Fslate.tar.gz&filename=Slate.tar.gz",
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(request.url, "https://example.com/releases/slate.tar.gz");
        assert_eq!(request.purpose, FetchPurpose::Navigation);
        assert_eq!(
            request.suggested_download_filename.as_deref(),
            Some("Slate.tar.gz")
        );
    }

    #[test]
    fn slate_download_request_supports_ipfs_and_suggests_path_filename() {
        let request = download_request_from_url(
            &Url::parse(
                "slate://download?url=ipfs%3A%2F%2FQmT5NvUtoM5nWFfrQdVrFtvGfKFmG7AHE8P34isapyhCxX%2Fimage.png",
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            request.url,
            "ipfs://QmT5NvUtoM5nWFfrQdVrFtvGfKFmG7AHE8P34isapyhCxX/image.png"
        );
        assert_eq!(
            request.suggested_download_filename.as_deref(),
            Some("image.png")
        );
    }

    #[test]
    fn slate_download_request_supports_tor_http() {
        let request = download_request_from_url(
            &Url::parse("slate://download?url=tor%2Bhttp%3A%2F%2Fexample.onion%2Ffile.zip")
                .unwrap(),
        )
        .unwrap();

        assert_eq!(request.url, "tor+http://example.onion/file.zip");
        assert_eq!(
            request.suggested_download_filename.as_deref(),
            Some("file.zip")
        );
    }

    #[test]
    fn slate_download_request_rejects_unsupported_targets() {
        let error = download_request_from_url(
            &Url::parse("slate://download?url=file%3A%2F%2F%2Ftmp%2Fsecret.txt").unwrap(),
        )
        .unwrap_err();

        assert_eq!(error, "unsupported download scheme: file");
    }

    #[test]
    fn slate_settings_url_matches_host_and_path_forms() {
        assert!(is_slate_settings_url(
            &Url::parse("slate://settings").unwrap()
        ));
        assert!(is_slate_settings_url(
            &Url::parse("slate:settings").unwrap()
        ));
        assert!(is_slate_settings_url(
            &Url::parse("slate://settings?chrome_zoom=0.9").unwrap()
        ));
        assert!(!is_slate_settings_url(&Url::parse("slate://home").unwrap()));
        assert!(!is_slate_settings_url(
            &Url::parse("https://settings").unwrap()
        ));
    }

    #[test]
    fn slate_settings_zoom_query_clamps_to_supported_range() {
        assert_eq!(
            chrome_element_zoom_setting_from_url(
                &Url::parse("slate://settings?chrome_zoom=0.82").unwrap()
            ),
            Some(0.82)
        );
        assert_eq!(
            chrome_element_zoom_setting_from_url(
                &Url::parse("slate://settings/apply?chrome_zoom=0.10").unwrap()
            ),
            Some(CHROME_ELEMENT_ZOOM_SETTING_MIN)
        );
        assert_eq!(
            chrome_element_zoom_setting_from_url(
                &Url::parse("slate://settings/apply?chrome_zoom=2.00").unwrap()
            ),
            Some(CHROME_ELEMENT_ZOOM_SETTING_MAX)
        );
        assert_eq!(
            chrome_element_zoom_setting_from_url(
                &Url::parse("slate://settings/preview?chrome_zoom=0.86").unwrap()
            ),
            Some(0.86)
        );
        assert_eq!(
            chrome_element_zoom_setting_from_url(
                &Url::parse("slate://settings/save?chrome_zoom=1.03").unwrap()
            ),
            Some(1.03)
        );
        assert_eq!(
            chrome_element_zoom_setting_from_url(
                &Url::parse("slate://home?chrome_zoom=0.82").unwrap()
            ),
            None
        );
    }

    #[test]
    fn slate_settings_zoom_action_urls_are_distinct() {
        assert!(is_slate_settings_preview_url(
            &Url::parse("slate://settings/preview?chrome_zoom=0.82").unwrap()
        ));
        assert!(is_slate_settings_save_url(
            &Url::parse("slate://settings/save?chrome_zoom=0.82").unwrap()
        ));
        assert!(is_slate_settings_apply_url(
            &Url::parse("slate://settings/apply?chrome_zoom=0.82").unwrap()
        ));
        assert!(!is_slate_settings_preview_url(
            &Url::parse("slate://settings/save?chrome_zoom=0.82").unwrap()
        ));
        assert!(!is_slate_settings_save_url(
            &Url::parse("slate://settings/preview?chrome_zoom=0.82").unwrap()
        ));
    }

    #[test]
    fn slate_settings_state_json_includes_key_bindings() {
        let parsed: serde_json::Value =
            serde_json::from_str(&super::settings_state_json(0.92)).unwrap();

        assert_eq!(parsed["chrome_zoom"], 0.92);
        assert_eq!(parsed["key_bindings"][0]["id"], "new_tab");
        assert_eq!(parsed["key_bindings"][0]["query"], "key_new_tab");
        assert_eq!(parsed["key_bindings"][1]["id"], "close_tab");
        assert_eq!(parsed["key_bindings"][2]["id"], "next_tab");
        assert_eq!(parsed["key_bindings"][3]["id"], "previous_tab");
        assert_eq!(parsed["key_bindings"][4]["id"], "next_app");
        assert_eq!(parsed["key_bindings"][4]["query"], "key_next_app");
        assert_eq!(parsed["key_bindings"][5]["id"], "previous_app");
        assert_eq!(parsed["key_bindings"][5]["query"], "key_previous_app");
        assert_eq!(parsed["key_bindings"][6]["id"], "cut");
        assert_eq!(parsed["key_bindings"][7]["id"], "copy");
        assert_eq!(parsed["key_bindings"][7]["query"], "key_copy");
        assert_eq!(parsed["key_bindings"][8]["id"], "paste");
        assert_eq!(parsed["key_bindings"][9]["id"], "select_all");
    }

    #[test]
    fn slate_internal_page_resources_exist() {
        let resource_dir = crate::resources::resource_protocol_dir_path();

        assert!(resource_dir.join("slate-blank.html").is_file());
        assert!(resource_dir.join("slate-home.html").is_file());
        assert!(resource_dir.join("slate-web.html").is_file());
        assert!(resource_dir.join("slate-calendar.html").is_file());
        assert!(resource_dir.join("slate-chat.html").is_file());
        assert!(resource_dir.join("slate-contacts.html").is_file());
        assert!(resource_dir.join("slate-files.html").is_file());
        assert!(resource_dir.join("slate-settings.html").is_file());
        assert!(resource_dir.join("slate-downloads.html").is_file());
        assert!(
            resource_dir
                .join("branding/slate-logo-cutout-256.png")
                .is_file()
        );
    }

    #[test]
    fn slate_home_and_web_pages_use_local_brand_asset() {
        let resource_dir = crate::resources::resource_protocol_dir_path();
        let home_page = std::fs::read_to_string(resource_dir.join("slate-home.html")).unwrap();
        let web_page = std::fs::read_to_string(resource_dir.join("slate-web.html")).unwrap();

        for page in [&home_page, &web_page] {
            assert!(page.contains("resource:///branding/slate-logo-cutout-256.png"));
            assert!(!page.contains("http://"));
            assert!(!page.contains("https://"));
        }

        assert!(home_page.contains("<title>Slate Home</title>"));
        assert!(web_page.contains("<title>Slate Web</title>"));
    }

    #[test]
    fn slate_calendar_page_is_static_local_mock() {
        let resource_dir = crate::resources::resource_protocol_dir_path();
        let calendar_page =
            std::fs::read_to_string(resource_dir.join("slate-calendar.html")).unwrap();

        assert!(calendar_page.contains("<title>Slate Calendar</title>"));
        assert!(calendar_page.contains("<h1>Calendar</h1>"));
        assert!(calendar_page.contains("August 2026"));
        assert!(!calendar_page.contains("<script"));
        assert!(!calendar_page.contains("http://"));
        assert!(!calendar_page.contains("https://"));
    }

    #[test]
    fn slate_chat_page_is_static_local_mock() {
        let resource_dir = crate::resources::resource_protocol_dir_path();
        let chat_page = std::fs::read_to_string(resource_dir.join("slate-chat.html")).unwrap();

        assert!(chat_page.contains("<title>Slate Chat</title>"));
        assert!(chat_page.contains("<h1>Chat</h1>"));
        assert!(chat_page.contains("SMS"));
        assert!(chat_page.contains("WhatsApp"));
        assert!(chat_page.contains("Future Providers"));
        assert!(!chat_page.contains("<script"));
        assert!(!chat_page.contains("http://"));
        assert!(!chat_page.contains("https://"));
    }

    #[test]
    fn slate_contacts_and_files_pages_are_static_local_mocks() {
        let resource_dir = crate::resources::resource_protocol_dir_path();
        let contacts_page =
            std::fs::read_to_string(resource_dir.join("slate-contacts.html")).unwrap();
        let files_page = std::fs::read_to_string(resource_dir.join("slate-files.html")).unwrap();

        assert!(contacts_page.contains("<title>Slate Contacts</title>"));
        assert!(contacts_page.contains("<h1>Contacts</h1>"));
        assert!(contacts_page.contains("Local address book"));
        assert!(!contacts_page.contains("<script"));
        assert!(!contacts_page.contains("http://"));
        assert!(!contacts_page.contains("https://"));

        assert!(files_page.contains("<title>Slate Files</title>"));
        assert!(files_page.contains("<h1>Files</h1>"));
        assert!(files_page.contains("Storage"));
        assert!(!files_page.contains("<script"));
        assert!(!files_page.contains("http://"));
        assert!(!files_page.contains("https://"));
    }

    #[test]
    fn slate_downloads_json_escapes_file_metadata() {
        let json = super::downloads_json(&[TemporaryDownloadRecord {
            profile: "default".to_string(),
            filename: "report \"final\".txt".to_string(),
            path: PathBuf::from("/tmp/report \"final\".txt"),
            size_bytes: 12,
        }]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let download = &parsed["downloads"][0];

        assert_eq!(download["profile"], "default");
        assert_eq!(download["filename"], "report \"final\".txt");
        assert_eq!(download["path"], "/tmp/report \"final\".txt");
        assert_eq!(download["size_bytes"], 12);
        assert_eq!(download["file_url"], "file:///tmp/report%20%22final%22.txt");
    }

    #[test]
    fn slate_download_error_html_escapes_message() {
        let html = slate_download_error_html("Download <Failed>", "bad <url>");

        assert!(html.contains("Download &lt;Failed&gt;"));
        assert!(html.contains("bad &lt;url&gt;"));
        assert!(html.contains("slate://downloads"));
    }

    #[test]
    fn slate_settings_page_previews_zoom_and_saves_explicitly() {
        let resource_dir = crate::resources::resource_protocol_dir_path();
        let settings_page =
            std::fs::read_to_string(resource_dir.join("slate-settings.html")).unwrap();
        let save_index = settings_page.find("id=\"save\"").unwrap();
        let reset_index = settings_page.find("id=\"reset\"").unwrap();

        assert!(save_index < reset_index);
        assert!(settings_page.contains("sendSetting(\"preview\""));
        assert!(settings_page.contains("sendSetting(\"save\""));
        assert!(settings_page.contains("Keyboard shortcuts"));
        assert!(settings_page.contains("id=\"save-shortcuts\""));
        assert!(settings_page.contains("key_new_tab"));
        assert!(settings_page.contains("key_next_tab"));
        assert!(settings_page.contains("key_previous_tab"));
        assert!(settings_page.contains("key_next_app"));
        assert!(settings_page.contains("key_previous_app"));
        assert!(settings_page.contains("key_copy"));
        assert!(settings_page.contains("key_select_all"));
        assert!(settings_page.contains("addShortcutCapture(input)"));
        assert!(settings_page.contains("Quote: \"'\""));
        assert!(settings_page.contains("Built-in shortcuts"));
        assert!(settings_page.contains("Primary+1 ... Primary+8"));
        assert!(settings_page.contains("Primary+R, F5"));
        assert!(settings_page.contains("Ctrl+F12"));
        assert!(!settings_page.contains("replaceState"));
        assert!(!settings_page.contains("type=\"range\""));
    }
}
