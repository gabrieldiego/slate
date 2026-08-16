/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Slate-owned internal browser pages.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use headers::{ContentType, HeaderMapExt};
use log::warn;
use servo::protocol_handler::{
    DoneChannel, FetchContext, NetworkError, ProtocolHandler, Request, ResourceFetchTiming,
    Response, ResponseBody,
};
use slate_broadwebd::{StateRoot, TemporaryDownloadRecord, default_session_state_root};
use slate_storage::{DEFAULT_PROFILE_ID, SlateProfileDatabase};
use url::Url;

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
            "home",
            "settings",
            "settings/state",
            "settings/preview",
            "settings/save",
            "settings/apply",
            "downloads",
            "downloads/state",
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
            return chrome_zoom_json_response(request, current_chrome_element_zoom_setting());
        }

        if is_slate_downloads_state_url(url.as_url()) {
            return downloads_json_response(request);
        }

        if is_slate_settings_preview_url(url.as_url()) {
            let zoom = chrome_element_zoom_setting_from_url(url.as_url())
                .map(set_current_chrome_element_zoom_setting)
                .unwrap_or_else(current_chrome_element_zoom_setting);
            return chrome_zoom_json_response(request, zoom);
        }

        if is_slate_settings_save_url(url.as_url()) || is_slate_settings_apply_url(url.as_url()) {
            let zoom = chrome_element_zoom_setting_from_url(url.as_url())
                .map(set_current_chrome_element_zoom_setting)
                .unwrap_or_else(current_chrome_element_zoom_setting);
            self.persist_chrome_element_zoom_setting(zoom);
            return chrome_zoom_json_response(request, zoom);
        }

        let resource_path = if is_slate_home_url(url.as_url()) {
            Some("/slate-home.html")
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
}

pub(crate) fn initialize_chrome_settings_from_database(database: &SlateProfileDatabase) {
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

fn chrome_zoom_json_response(
    request: &Request,
    zoom: f32,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(format!("{{\"chrome_zoom\":{zoom:.2}}}").into());
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

pub(crate) fn is_slate_home_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("home") || url.path().trim_start_matches('/') == "home")
}

pub(crate) fn is_slate_downloads_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("downloads")
            || url.path().trim_start_matches('/') == "downloads")
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
        chrome_element_zoom_setting_from_url, is_slate_downloads_state_url, is_slate_downloads_url,
        is_slate_home_url, is_slate_settings_apply_url, is_slate_settings_preview_url,
        is_slate_settings_save_url, is_slate_settings_url,
    };
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
    fn slate_internal_page_resources_exist() {
        let resource_dir = crate::resources::resource_protocol_dir_path();

        assert!(resource_dir.join("slate-home.html").is_file());
        assert!(resource_dir.join("slate-settings.html").is_file());
        assert!(resource_dir.join("slate-downloads.html").is_file());
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
    fn slate_settings_page_previews_zoom_and_saves_explicitly() {
        let resource_dir = crate::resources::resource_protocol_dir_path();
        let settings_page =
            std::fs::read_to_string(resource_dir.join("slate-settings.html")).unwrap();
        let save_index = settings_page.find("id=\"save\"").unwrap();
        let reset_index = settings_page.find("id=\"reset\"").unwrap();

        assert!(save_index < reset_index);
        assert!(settings_page.contains("sendSetting(\"preview\""));
        assert!(settings_page.contains("sendSetting(\"save\""));
        assert!(!settings_page.contains("replaceState"));
        assert!(!settings_page.contains("type=\"range\""));
    }
}
