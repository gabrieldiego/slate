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
use slate_storage::SlateProfileDatabase;
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
        &["home", "settings", "settings/state", "settings/apply"]
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

        if is_slate_settings_apply_url(url.as_url()) {
            let zoom = chrome_element_zoom_setting_from_url(url.as_url())
                .map(set_current_chrome_element_zoom_setting)
                .unwrap_or_else(current_chrome_element_zoom_setting);
            self.persist_chrome_element_zoom_setting(zoom);
            return chrome_zoom_json_response(request, zoom);
        }

        let resource_path = if is_slate_home_url(url.as_url()) {
            Some("/slate-home.html")
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

pub(crate) fn is_slate_home_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("home") || url.path().trim_start_matches('/') == "home")
}

pub(crate) fn is_slate_settings_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("settings") || url.path().trim_start_matches('/') == "settings")
}

fn is_slate_settings_state_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "state"
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
        chrome_element_zoom_setting_from_url, is_slate_home_url, is_slate_settings_url,
    };
    use url::Url;

    #[test]
    fn slate_home_url_matches_host_and_path_forms() {
        assert!(is_slate_home_url(&Url::parse("slate://home").unwrap()));
        assert!(is_slate_home_url(&Url::parse("slate:home").unwrap()));
        assert!(!is_slate_home_url(&Url::parse("slate://settings").unwrap()));
        assert!(!is_slate_home_url(&Url::parse("https://home").unwrap()));
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
                &Url::parse("slate://home?chrome_zoom=0.82").unwrap()
            ),
            None
        );
    }

    #[test]
    fn slate_internal_page_resources_exist() {
        let resource_dir = crate::resources::resource_protocol_dir_path();

        assert!(resource_dir.join("slate-home.html").is_file());
        assert!(resource_dir.join("slate-settings.html").is_file());
    }
}
