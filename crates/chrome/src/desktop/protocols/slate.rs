/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Slate-owned internal browser pages.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use headers::{ContentType, HeaderMapExt, HeaderName, HeaderValue};
use log::warn;
use servo::ServoUrl;
use servo::protocol_handler::{
    DoneChannel, FetchContext, HttpStatus, NetworkError, ProtocolHandler, Request,
    ResourceFetchTiming, Response, ResponseBody,
};
use slate_broadwebd::{
    DownloadRecord, FetchDisposition, HttpFetchRequest, HttpFetchResponse,
    IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX, StateRoot, TemporaryDownloadRecord,
    default_session_state_root,
};
use slate_profile_sync::{
    LocalSettingsSyncCurrentCycleReport, LocalSettingsSyncPreviewCycleReport,
    LocalSettingsSyncPreviewError, LocalSettingsSyncTwoDevicePreviewCycleReport,
    local_settings_sync_current_cycle_report_json, local_settings_sync_preview_cycle_report_json,
    local_settings_sync_two_device_preview_cycle_report_json,
    run_local_settings_sync_current_cycle, run_local_settings_sync_preview_cycle,
    run_local_settings_sync_two_device_preview_cycle,
};
use slate_storage::{
    DEFAULT_PROFILE_ID, DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID,
    PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES, ProfileSyncLocalReadinessReport,
    ProfileSyncLocalSecretActivationRecord, ProfileSyncSecretHandoffApplication,
    ProfileSyncSecretHandoffBundle, SYNC_DOMAIN_SETTINGS, SlateProfileDatabase, SlateSyncSecret,
    SlateSyncSecretExport, StorageError, SyncObjectError, SyncSettingTextEvent,
};
use url::Url;

use crate::desktop::key_bindings::{
    SlateKeyBindings, apply_key_binding_setting, current_key_bindings_json_value,
    initialize_key_bindings_from_database, key_bindings_from_settings_url,
    persist_key_bindings_to_database, set_current_key_bindings,
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

pub struct SlateProtocolHandler {
    database: Option<SlateProfileDatabase>,
    profile_sync_preview: Arc<Mutex<ProfileSyncPreviewState>>,
}

impl Default for SlateProtocolHandler {
    fn default() -> Self {
        Self {
            database: None,
            profile_sync_preview: Arc::default(),
        }
    }
}

impl SlateProtocolHandler {
    pub(crate) fn new(database: SlateProfileDatabase) -> Self {
        initialize_chrome_settings_from_database(&database);
        Self {
            database: Some(database),
            profile_sync_preview: Arc::default(),
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
            "settings/profile-sync/state",
            "settings/profile-sync/create",
            "settings/profile-sync/check",
            "settings/profile-sync/local-provider",
            "settings/profile-sync/key/export",
            "settings/profile-sync/key/import",
            "settings/profile-sync/handoff/create",
            "settings/profile-sync/handoff/import",
            "settings/profile-sync/run-current",
            "settings/profile-sync/run-local",
            "settings/profile-sync/run-local-two-device",
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

        if is_slate_settings_profile_sync_state_url(url.as_url()) {
            return self.profile_sync_preview_json_response(request);
        }

        if is_slate_settings_profile_sync_create_url(url.as_url()) {
            return self.create_profile_sync_preview_secret_response(request);
        }

        if is_slate_settings_profile_sync_check_url(url.as_url()) {
            return self.check_profile_sync_preview_response(request);
        }

        if is_slate_settings_profile_sync_local_provider_url(url.as_url()) {
            return self.activate_profile_sync_preview_provider_response(request);
        }

        if is_slate_settings_profile_sync_key_export_url(url.as_url()) {
            return self.export_profile_sync_key_response(request, url.as_url());
        }

        if is_slate_settings_profile_sync_key_import_url(url.as_url()) {
            return self.import_profile_sync_key_response(request, url.as_url());
        }

        if is_slate_settings_profile_sync_handoff_create_url(url.as_url()) {
            return self.create_profile_sync_secret_handoff_bundle_response(request, url.as_url());
        }

        if is_slate_settings_profile_sync_handoff_import_url(url.as_url()) {
            return self.import_profile_sync_secret_handoff_bundle_response(request, url.as_url());
        }

        if is_slate_settings_profile_sync_run_current_url(url.as_url()) {
            return self.run_profile_sync_preview_current_sync_response(request);
        }

        if is_slate_settings_profile_sync_run_local_url(url.as_url()) {
            return self.run_profile_sync_preview_local_trial_response(request);
        }

        if is_slate_settings_profile_sync_run_local_two_device_url(url.as_url()) {
            return self.run_profile_sync_preview_two_device_local_trial_response(request);
        }

        if is_slate_downloads_state_url(url.as_url()) {
            return downloads_json_response(request);
        }

        if is_slate_download_request_url(url.as_url()) {
            return self.download_url_response(request);
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
    fn download_url_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let request_url = request.current_url();
        let timing = ResourceFetchTiming::new(request.timing_type());
        let response = match profile_sync_key_download_filename_from_url(request_url.as_url()) {
            Ok(Some(filename)) => {
                self.profile_sync_key_download_response(request_url, timing, filename.as_str())
            }
            Ok(None) => match download_request_from_url(request_url.as_url()) {
                Ok(fetch_request) => match fetch_with_default_broadwebd(fetch_request) {
                    Ok(fetch_response) => {
                        download_fetch_response(request_url, timing, fetch_response)
                    }
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

    fn profile_sync_key_download_response(
        &self,
        request_url: ServoUrl,
        timing: ResourceFetchTiming,
        filename: &str,
    ) -> Response {
        let export = match self.profile_sync_key_export() {
            Ok(export) => export,
            Err((status_code, error)) => {
                return slate_download_error_response(
                    request_url,
                    timing,
                    "Download Failed",
                    &error,
                    status_code,
                );
            }
        };
        let body = match export.to_bytes() {
            Ok(body) => body,
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error.to_string());
                return slate_download_error_response(
                    request_url,
                    timing,
                    "Download Failed",
                    &error.to_string(),
                    500,
                );
            }
        };
        let state_root = match StateRoot::prepare(default_session_state_root()) {
            Ok(state_root) => state_root,
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error.to_string());
                return slate_download_error_response(
                    request_url,
                    timing,
                    "Download Failed",
                    &error.to_string(),
                    500,
                );
            }
        };
        let path = match state_root.store_download(DEFAULT_PROFILE_ID, filename, body.as_slice()) {
            Ok(path) => path,
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error.to_string());
                return slate_download_error_response(
                    request_url,
                    timing,
                    "Download Failed",
                    &error.to_string(),
                    500,
                );
            }
        };
        let saved_filename = path
            .file_name()
            .and_then(|filename| filename.to_str())
            .unwrap_or(filename)
            .to_string();
        let download = DownloadRecord::new(
            DEFAULT_PROFILE_ID,
            saved_filename,
            path,
            body.len(),
            Some("application/json".to_string()),
        );
        let fetch_response = HttpFetchResponse::new(
            "slate://settings/profile-sync/key/export",
            200,
            Some("application/json".to_string()),
            Vec::new(),
            body,
        )
        .with_download_disposition(filename)
        .with_download(download);
        let mut state = self.profile_sync_preview.lock().unwrap();
        state.last_error = None;
        download_fetch_response(request_url, timing, fetch_response)
    }

    fn profile_sync_key_export(&self) -> Result<SlateSyncSecretExport, (u16, String)> {
        let mut state = self.profile_sync_preview.lock().unwrap();
        self.refresh_profile_sync_preview_metadata(&mut state);
        match state.active_export.clone() {
            Some(export) => {
                state.last_error = None;
                Ok(export)
            }
            None if !state.metadata_ready => {
                let created = state.create_secret_export_for_unenrolled_profile(
                    DEFAULT_PROFILE_ID,
                    unix_time_seconds(),
                );
                let (sync_secret, export) = match created {
                    Ok(created) => created,
                    Err(error) => {
                        let message = error.to_string();
                        state.last_error = Some(message.clone());
                        return Err((500, message));
                    }
                };
                match self.activate_profile_sync_preview_from_secret(&sync_secret) {
                    Ok(Some(activation)) => {
                        state.mark_secret_activation_ready(&activation);
                        state.last_error = None;
                        Ok(export)
                    }
                    Ok(None) => {
                        let message = "settings database is not available".to_string();
                        state.last_error = Some(message.clone());
                        Err((500, message))
                    }
                    Err(error) => {
                        let message = error.to_string();
                        state.last_error = Some(message.clone());
                        Err((500, message))
                    }
                }
            }
            None => {
                let message =
                    "profile enrollment key is not loaded on this device; import it before downloading it again"
                        .to_string();
                state.last_error = Some(message.clone());
                Err((400, message))
            }
        }
    }

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

    fn profile_sync_preview_json_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let mut state = self.profile_sync_preview.lock().unwrap();
        self.refresh_profile_sync_preview_metadata(&mut state);
        match self.profile_sync_local_readiness_report() {
            Ok(readiness) => json_response(request, 200, state.to_json(readiness.as_ref())),
            Err(error) => {
                state.last_error = Some(error.to_string());
                json_response(request, 500, state.to_json(None))
            }
        }
    }

    fn create_profile_sync_preview_secret_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let mut state = self.profile_sync_preview.lock().unwrap();
        match state.create_secret(DEFAULT_PROFILE_ID, unix_time_seconds()) {
            Ok(sync_secret) => match self.activate_profile_sync_preview_from_secret(&sync_secret) {
                Ok(Some(activation)) => {
                    state.mark_secret_activation_ready(&activation);
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    json_response(request, 200, state.to_json(readiness.as_ref()))
                }
                Ok(None) => json_response(request, 200, state.to_json(None)),
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    json_response(request, 500, state.to_json(None))
                }
            },
            Err(error) => {
                state.last_error = Some(error.to_string());
                json_response(request, 500, state.to_json(None))
            }
        }
    }

    fn check_profile_sync_preview_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let mut state = self.profile_sync_preview.lock().unwrap();
        self.refresh_profile_sync_preview_metadata(&mut state);
        match self.profile_sync_local_readiness_report() {
            Ok(readiness) => {
                state.last_error = None;
                json_response(request, 200, state.to_json(readiness.as_ref()))
            }
            Err(error) => {
                state.last_error = Some(error.to_string());
                json_response(request, 500, state.to_json(None))
            }
        }
    }

    fn activate_profile_sync_preview_provider_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let sync_secret = {
            let mut state = self.profile_sync_preview.lock().unwrap();
            self.refresh_profile_sync_preview_metadata(&mut state);
            match state.active_sync_secret(DEFAULT_PROFILE_ID) {
                Ok(Some(sync_secret)) => sync_secret,
                Ok(None) => {
                    state.last_error = Some(
                        "create a local profile or import an enrollment file before adding a provider"
                            .to_string(),
                    );
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    return json_response(request, 400, state.to_json(None));
                }
            }
        };
        let mut state = self.profile_sync_preview.lock().unwrap();
        match self.activate_profile_sync_preview_provider(&sync_secret) {
            Ok(()) => match self.profile_sync_local_readiness_report() {
                Ok(readiness) => {
                    state.last_error = None;
                    json_response(request, 200, state.to_json(readiness.as_ref()))
                }
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    json_response(request, 500, state.to_json(None))
                }
            },
            Err(error) => {
                state.last_error = Some(error.to_string());
                json_response(request, 500, state.to_json(None))
            }
        }
    }

    fn export_profile_sync_key_response(
        &self,
        request: &Request,
        url: &Url,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let export = match self.profile_sync_key_export() {
            Ok(export) => export,
            Err((status_code, _)) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                return json_response(request, status_code, state.to_json(readiness.as_ref()));
            }
        };
        let mut state = self.profile_sync_preview.lock().unwrap();
        let readiness = self.profile_sync_local_readiness_report().ok().flatten();
        state.last_error = None;
        if profile_sync_download_requested(url) {
            match export.to_bytes() {
                Ok(bytes) => profile_sync_json_attachment_response(
                    request,
                    profile_sync_key_filename(export.profile.as_str()).as_str(),
                    bytes,
                ),
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    json_response(request, 500, state.to_json(readiness.as_ref()))
                }
            }
        } else {
            json_response(
                request,
                200,
                state.to_json_with_key_export(readiness.as_ref()),
            )
        }
    }

    fn import_profile_sync_key_response(
        &self,
        request: &Request,
        url: &Url,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let key_text = match profile_sync_key_text_from_url(url) {
            Ok(Some(key_text)) => key_text,
            Ok(None) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some("missing profile sync enrollment key contents".to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                return json_response(request, 400, state.to_json(readiness.as_ref()));
            }
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error);
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                return json_response(request, 413, state.to_json(readiness.as_ref()));
            }
        };

        let sync_secret = {
            let mut state = self.profile_sync_preview.lock().unwrap();
            match state.import_secret(DEFAULT_PROFILE_ID, key_text.as_str()) {
                Ok(sync_secret) => sync_secret,
                Err(secret_error) => {
                    state.last_error = Some(secret_error.to_string());
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
            }
        };

        match self.activate_profile_sync_preview_from_secret(&sync_secret) {
            Ok(Some(activation)) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.mark_secret_activation_ready(&activation);
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 200, state.to_json(readiness.as_ref()))
            }
            Ok(None) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some("settings database is not available".to_string());
                json_response(request, 500, state.to_json(None))
            }
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 500, state.to_json(readiness.as_ref()))
            }
        }
    }

    fn create_profile_sync_secret_handoff_bundle_response(
        &self,
        request: &Request,
        url: &Url,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let target_device_id = profile_sync_handoff_target_device_id_from_url(url);
        let sync_secret = {
            let mut state = self.profile_sync_preview.lock().unwrap();
            self.refresh_profile_sync_preview_metadata(&mut state);
            match (
                target_device_id.as_deref(),
                state.active_sync_secret(DEFAULT_PROFILE_ID),
            ) {
                (None, _) => {
                    state.last_error = Some(
                        "enter a target device id before downloading an enrollment file"
                            .to_string(),
                    );
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
                (_, Ok(Some(sync_secret))) => sync_secret,
                (_, Ok(None)) => {
                    state.last_error = Some(
                        "create a local profile before downloading an enrollment file".to_string(),
                    );
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
                (_, Err(error)) => {
                    state.last_error = Some(error.to_string());
                    return json_response(request, 400, state.to_json(None));
                }
            }
        };
        let target_device_id = target_device_id.expect("validated target device id");
        let bundle = SlateProfileDatabase::profile_sync_secret_handoff_bundle_from_secret(
            DEFAULT_PROFILE_ID,
            &sync_secret,
            target_device_id.as_str(),
        );
        let mut state = self.profile_sync_preview.lock().unwrap();
        match bundle {
            Ok(bundle) => {
                state.mark_secret_handoff_bundle(&bundle);
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(
                    request,
                    200,
                    state.to_json_with_handoff_export(readiness.as_ref()),
                )
            }
            Err(error) => {
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 400, state.to_json(readiness.as_ref()))
            }
        }
    }

    fn import_profile_sync_secret_handoff_bundle_response(
        &self,
        request: &Request,
        url: &Url,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let bundle_text = match profile_sync_handoff_bundle_text_from_url(url) {
            Ok(Some(bundle_text)) => bundle_text,
            Ok(None) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error =
                    Some("missing profile sync enrollment file contents".to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                return json_response(request, 400, state.to_json(readiness.as_ref()));
            }
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error);
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                return json_response(request, 413, state.to_json(readiness.as_ref()));
            }
        };
        let bundle = match ProfileSyncSecretHandoffBundle::from_bytes(bundle_text.as_bytes()) {
            Ok(bundle) => bundle,
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                return json_response(request, 400, state.to_json(readiness.as_ref()));
            }
        };
        match self.apply_profile_sync_secret_handoff_bundle(&bundle) {
            Ok(application) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.mark_secret_handoff_application(&bundle, &application);
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 200, state.to_json(readiness.as_ref()))
            }
            Err(error) => {
                let mut state = self.profile_sync_preview.lock().unwrap();
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 400, state.to_json(readiness.as_ref()))
            }
        }
    }

    fn run_profile_sync_preview_current_sync_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let sync_secret = {
            let mut state = self.profile_sync_preview.lock().unwrap();
            self.refresh_profile_sync_preview_metadata(&mut state);
            match state.active_sync_secret(DEFAULT_PROFILE_ID) {
                Ok(Some(sync_secret)) => sync_secret,
                Ok(None) => {
                    state.last_error = Some(
                        "create a local profile or import an enrollment file before syncing current settings"
                            .to_string(),
                    );
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    return json_response(request, 400, state.to_json(None));
                }
            }
        };
        let run = self.run_profile_sync_preview_current_sync(&sync_secret);
        let mut state = self.profile_sync_preview.lock().unwrap();
        match run {
            Ok(Some(report)) => {
                state.mark_current_sync_result(&report, unix_time_seconds());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 200, state.to_json(readiness.as_ref()))
            }
            Ok(None) => {
                state.last_error = Some("settings database is not available".to_string());
                json_response(request, 500, state.to_json(None))
            }
            Err(error) => {
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 500, state.to_json(readiness.as_ref()))
            }
        }
    }

    fn run_profile_sync_preview_local_trial_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let sync_secret = {
            let mut state = self.profile_sync_preview.lock().unwrap();
            self.refresh_profile_sync_preview_metadata(&mut state);
            match state.active_sync_secret(DEFAULT_PROFILE_ID) {
                Ok(Some(sync_secret)) => sync_secret,
                Ok(None) => {
                    state.last_error = Some(
                        "create a local profile or import an enrollment file before running a local trial"
                            .to_string(),
                    );
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    return json_response(request, 400, state.to_json(None));
                }
            }
        };
        let trial = self.run_profile_sync_preview_local_trial(&sync_secret);
        let mut state = self.profile_sync_preview.lock().unwrap();
        match trial {
            Ok(Some(report)) => {
                state.mark_trial_result(&report, unix_time_seconds());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 200, state.to_json(readiness.as_ref()))
            }
            Ok(None) => {
                state.last_error = Some("settings database is not available".to_string());
                json_response(request, 500, state.to_json(None))
            }
            Err(error) => {
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 500, state.to_json(readiness.as_ref()))
            }
        }
    }

    fn run_profile_sync_preview_two_device_local_trial_response(
        &self,
        request: &Request,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let sync_secret = {
            let mut state = self.profile_sync_preview.lock().unwrap();
            self.refresh_profile_sync_preview_metadata(&mut state);
            match state.active_sync_secret(DEFAULT_PROFILE_ID) {
                Ok(Some(sync_secret)) => sync_secret,
                Ok(None) => {
                    state.last_error = Some(
                        "create a local profile or import an enrollment file before running a two-device trial"
                            .to_string(),
                    );
                    let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                    return json_response(request, 400, state.to_json(readiness.as_ref()));
                }
                Err(error) => {
                    state.last_error = Some(error.to_string());
                    return json_response(request, 400, state.to_json(None));
                }
            }
        };
        let trial = self.run_profile_sync_preview_two_device_local_trial(&sync_secret);
        let mut state = self.profile_sync_preview.lock().unwrap();
        match trial {
            Ok(Some(report)) => {
                state.mark_two_device_trial_result(&report, unix_time_seconds());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 200, state.to_json(readiness.as_ref()))
            }
            Ok(None) => {
                state.last_error = Some("settings database is not available".to_string());
                json_response(request, 500, state.to_json(None))
            }
            Err(error) => {
                state.last_error = Some(error.to_string());
                let readiness = self.profile_sync_local_readiness_report().ok().flatten();
                json_response(request, 500, state.to_json(readiness.as_ref()))
            }
        }
    }

    fn activate_profile_sync_preview_from_secret(
        &self,
        sync_secret: &SlateSyncSecret,
    ) -> Result<Option<ProfileSyncLocalSecretActivationRecord>, StorageError> {
        self.database
            .as_ref()
            .map(|database| {
                database.activate_local_profile_sync_from_secret(DEFAULT_PROFILE_ID, sync_secret)
            })
            .transpose()
    }

    fn activate_profile_sync_preview_provider(
        &self,
        sync_secret: &SlateSyncSecret,
    ) -> Result<(), StorageError> {
        if let Some(database) = &self.database {
            let endpoint_ref = format!(
                "{IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX}preview/{DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID}"
            );
            database.activate_local_profile_sync_preview_provider_from_secret(
                DEFAULT_PROFILE_ID,
                sync_secret,
                Some(endpoint_ref),
            )?;
        }
        Ok(())
    }

    fn apply_profile_sync_secret_handoff_bundle(
        &self,
        bundle: &ProfileSyncSecretHandoffBundle,
    ) -> Result<ProfileSyncSecretHandoffApplication, StorageError> {
        self.database
            .as_ref()
            .map(|database| database.apply_profile_sync_secret_handoff_bundle(bundle))
            .unwrap_or_else(|| {
                Err(StorageError::InvalidProfileSyncSecretHandoffBundle(
                    "settings database is not available".to_string(),
                ))
            })
    }

    fn run_profile_sync_preview_current_sync(
        &self,
        sync_secret: &SlateSyncSecret,
    ) -> Result<Option<LocalSettingsSyncCurrentCycleReport>, LocalSettingsSyncPreviewError> {
        self.database
            .as_ref()
            .map(|database| {
                run_local_settings_sync_current_cycle(
                    database,
                    DEFAULT_PROFILE_ID,
                    sync_secret,
                    default_session_state_root().join("profile-sync-preview"),
                )
            })
            .transpose()
    }

    fn run_profile_sync_preview_local_trial(
        &self,
        sync_secret: &SlateSyncSecret,
    ) -> Result<Option<LocalSettingsSyncPreviewCycleReport>, LocalSettingsSyncPreviewError> {
        self.database
            .as_ref()
            .map(|database| {
                run_local_settings_sync_preview_cycle(
                    database,
                    DEFAULT_PROFILE_ID,
                    sync_secret,
                    default_session_state_root().join("profile-sync-preview"),
                )
            })
            .transpose()
    }

    fn run_profile_sync_preview_two_device_local_trial(
        &self,
        sync_secret: &SlateSyncSecret,
    ) -> Result<Option<LocalSettingsSyncTwoDevicePreviewCycleReport>, LocalSettingsSyncPreviewError>
    {
        self.database
            .as_ref()
            .map(|database| {
                run_local_settings_sync_two_device_preview_cycle(
                    database,
                    DEFAULT_PROFILE_ID,
                    sync_secret,
                    default_session_state_root().join("profile-sync-preview"),
                )
            })
            .transpose()
    }

    fn profile_sync_local_readiness_report(
        &self,
    ) -> Result<Option<ProfileSyncLocalReadinessReport>, StorageError> {
        self.database
            .as_ref()
            .map(|database| database.profile_sync_local_readiness(DEFAULT_PROFILE_ID))
            .transpose()
    }

    fn refresh_profile_sync_preview_metadata(&self, state: &mut ProfileSyncPreviewState) {
        let Some(database) = &self.database else {
            return;
        };
        state.local_device_id = Some(database.local_sync_device_id().to_string());
        if state.metadata_ready {
            return;
        }
        let Ok(Some(content_key_epoch)) =
            database.active_sync_content_key_epoch(DEFAULT_PROFILE_ID)
        else {
            return;
        };
        state.metadata_ready = true;
        state.active_key_id = Some(content_key_epoch.key_id);
        state.local_device_id = Some(database.local_sync_device_id().to_string());
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ProfileSyncPreviewState {
    active_export: Option<SlateSyncSecretExport>,
    metadata_ready: bool,
    active_key_id: Option<String>,
    local_device_id: Option<String>,
    active_handoff_bundle: Option<ProfileSyncSecretHandoffBundle>,
    last_current_sync: Option<ProfileSyncPreviewCurrentSyncState>,
    last_trial: Option<ProfileSyncPreviewTrialState>,
    last_two_device_trial: Option<ProfileSyncPreviewTwoDeviceTrialState>,
    last_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProfileSyncPreviewCurrentSyncState {
    completed_at: i64,
    report: LocalSettingsSyncCurrentCycleReport,
}

impl ProfileSyncPreviewCurrentSyncState {
    fn from_report(report: &LocalSettingsSyncCurrentCycleReport, completed_at: i64) -> Self {
        Self {
            completed_at,
            report: report.clone(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        local_settings_sync_current_cycle_report_json(&self.report, self.completed_at)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProfileSyncPreviewTrialState {
    completed_at: i64,
    report: LocalSettingsSyncPreviewCycleReport,
}

impl ProfileSyncPreviewTrialState {
    fn from_report(report: &LocalSettingsSyncPreviewCycleReport, completed_at: i64) -> Self {
        Self {
            completed_at,
            report: report.clone(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        local_settings_sync_preview_cycle_report_json(&self.report, self.completed_at)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProfileSyncPreviewTwoDeviceTrialState {
    completed_at: i64,
    report: LocalSettingsSyncTwoDevicePreviewCycleReport,
}

impl ProfileSyncPreviewTwoDeviceTrialState {
    fn from_report(
        report: &LocalSettingsSyncTwoDevicePreviewCycleReport,
        completed_at: i64,
    ) -> Self {
        Self {
            completed_at,
            report: report.clone(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        local_settings_sync_two_device_preview_cycle_report_json(&self.report, self.completed_at)
    }
}

impl ProfileSyncPreviewState {
    fn create_secret(
        &mut self,
        profile: &str,
        created_at: i64,
    ) -> Result<SlateSyncSecret, SyncObjectError> {
        let secret = SlateSyncSecret::generate()?;
        self.active_export = Some(secret.export_for_profile(profile, created_at));
        self.metadata_ready = false;
        self.active_key_id = None;
        self.local_device_id = None;
        self.active_handoff_bundle = None;
        self.last_current_sync = None;
        self.last_trial = None;
        self.last_two_device_trial = None;
        self.last_error = None;
        Ok(secret)
    }

    fn create_secret_export_for_unenrolled_profile(
        &mut self,
        profile: &str,
        created_at: i64,
    ) -> Result<(SlateSyncSecret, SlateSyncSecretExport), SyncObjectError> {
        let sync_secret = self.create_secret(profile, created_at)?;
        let export = self
            .active_export
            .clone()
            .expect("create_secret stores an active export");
        Ok((sync_secret, export))
    }

    fn import_secret(
        &mut self,
        expected_profile: &str,
        export_text: &str,
    ) -> Result<SlateSyncSecret, SyncObjectError> {
        let export = SlateSyncSecretExport::from_bytes(export_text.as_bytes())?;
        let sync_secret = SlateSyncSecret::from_export_for_profile(&export, expected_profile)?;
        self.active_export = Some(export);
        self.metadata_ready = false;
        self.active_key_id = None;
        self.local_device_id = None;
        self.active_handoff_bundle = None;
        self.last_current_sync = None;
        self.last_trial = None;
        self.last_two_device_trial = None;
        self.last_error = None;
        Ok(sync_secret)
    }

    fn active_sync_secret(
        &self,
        expected_profile: &str,
    ) -> Result<Option<SlateSyncSecret>, SyncObjectError> {
        self.active_export
            .as_ref()
            .map(|export| SlateSyncSecret::from_export_for_profile(export, expected_profile))
            .transpose()
    }

    fn mark_secret_activation_ready(
        &mut self,
        activation: &ProfileSyncLocalSecretActivationRecord,
    ) {
        self.metadata_ready = true;
        self.active_key_id = Some(activation.activation.content_key_epoch.key_id.clone());
        self.local_device_id = Some(activation.local_device_id.clone());
        self.last_error = None;
    }

    fn mark_current_sync_result(
        &mut self,
        report: &LocalSettingsSyncCurrentCycleReport,
        completed_at: i64,
    ) {
        self.metadata_ready = true;
        self.local_device_id = Some(report.local_device_id.clone());
        self.last_current_sync = Some(ProfileSyncPreviewCurrentSyncState::from_report(
            report,
            completed_at,
        ));
        self.last_error = None;
    }

    fn mark_trial_result(
        &mut self,
        report: &LocalSettingsSyncPreviewCycleReport,
        completed_at: i64,
    ) {
        self.metadata_ready = true;
        self.local_device_id = Some(report.local_device_id.clone());
        self.last_trial = Some(ProfileSyncPreviewTrialState::from_report(
            report,
            completed_at,
        ));
        self.last_error = None;
    }

    fn mark_two_device_trial_result(
        &mut self,
        report: &LocalSettingsSyncTwoDevicePreviewCycleReport,
        completed_at: i64,
    ) {
        self.metadata_ready = true;
        self.local_device_id = Some(report.publisher_device_id.clone());
        self.last_two_device_trial = Some(ProfileSyncPreviewTwoDeviceTrialState::from_report(
            report,
            completed_at,
        ));
        self.last_error = None;
    }

    fn mark_secret_handoff_bundle(&mut self, bundle: &ProfileSyncSecretHandoffBundle) {
        self.active_handoff_bundle = Some(bundle.clone());
        self.last_error = None;
    }

    fn mark_secret_handoff_application(
        &mut self,
        bundle: &ProfileSyncSecretHandoffBundle,
        application: &ProfileSyncSecretHandoffApplication,
    ) {
        self.active_export = Some(bundle.sync_secret_export.clone());
        self.active_handoff_bundle = Some(bundle.clone());
        self.mark_secret_activation_ready(&application.activation);
    }

    fn to_json(&self, readiness: Option<&ProfileSyncLocalReadinessReport>) -> String {
        self.to_json_with_exports(readiness, false, false)
    }

    fn to_json_with_handoff_export(
        &self,
        readiness: Option<&ProfileSyncLocalReadinessReport>,
    ) -> String {
        self.to_json_with_exports(readiness, true, false)
    }

    fn to_json_with_key_export(
        &self,
        readiness: Option<&ProfileSyncLocalReadinessReport>,
    ) -> String {
        self.to_json_with_exports(readiness, false, true)
    }

    fn to_json_with_exports(
        &self,
        readiness: Option<&ProfileSyncLocalReadinessReport>,
        include_handoff_export: bool,
        include_key_export: bool,
    ) -> String {
        let key_export_text = if include_key_export {
            self.active_export
                .as_ref()
                .and_then(|export| export.to_bytes().ok())
                .and_then(|bytes| String::from_utf8(bytes).ok())
        } else {
            None
        };
        let key_export_filename = self
            .active_export
            .as_ref()
            .map(|export| profile_sync_key_filename(export.profile.as_str()));
        let handoff_export_text = if include_handoff_export {
            self.active_handoff_bundle
                .as_ref()
                .and_then(|bundle| bundle.to_bytes().ok())
                .and_then(|bytes| String::from_utf8(bytes).ok())
        } else {
            None
        };
        let handoff_target_device_id = self
            .active_handoff_bundle
            .as_ref()
            .map(|bundle| bundle.target_device_id.as_str());
        let handoff_filename = self
            .active_handoff_bundle
            .as_ref()
            .map(|bundle| profile_sync_handoff_filename(bundle.target_device_id.as_str()));
        let mut state_json = serde_json::json!({
            "profile": DEFAULT_PROFILE_ID,
            "status": if self.active_export.is_some() {
                "ready"
            } else if self.metadata_ready {
                "metadata_ready"
            } else {
                "not_enrolled"
            },
            "has_secret": self.active_export.is_some(),
            "metadata_ready": self.metadata_ready,
            "active_key_id": self.active_key_id.as_deref(),
            "local_device_id": self.local_device_id.as_deref(),
            "key_export_filename": key_export_filename.as_deref(),
            "local_sync": readiness.map(profile_sync_local_readiness_json),
            "last_current_sync": self.last_current_sync.as_ref().map(ProfileSyncPreviewCurrentSyncState::to_json),
            "last_trial": self.last_trial.as_ref().map(ProfileSyncPreviewTrialState::to_json),
            "last_two_device_trial": self.last_two_device_trial.as_ref().map(ProfileSyncPreviewTwoDeviceTrialState::to_json),
            "handoff_export_filename": handoff_filename.as_deref(),
            "handoff_target_device_id": handoff_target_device_id,
            "last_error": self.last_error.as_deref(),
        });
        if include_handoff_export
            && let Some(handoff_export_text) = handoff_export_text
            && let Some(object) = state_json.as_object_mut()
        {
            object.insert(
                "handoff_export_text".to_string(),
                serde_json::Value::String(handoff_export_text),
            );
        }
        if include_key_export
            && let Some(key_export_text) = key_export_text
            && let Some(object) = state_json.as_object_mut()
        {
            object.insert(
                "key_export_text".to_string(),
                serde_json::Value::String(key_export_text),
            );
        }
        state_json.to_string()
    }
}

fn profile_sync_local_readiness_json(
    readiness: &ProfileSyncLocalReadinessReport,
) -> serde_json::Value {
    serde_json::json!({
        "profile": readiness.profile.as_str(),
        "local_device_id": readiness.local_device_id.as_str(),
        "local_device_registered": readiness.local_device_registered,
        "local_device_trusted": readiness.local_device_trusted,
        "account_authority_trusted": readiness.account_authority_trusted,
        "trusted_device_count": readiness.trusted_device_count,
        "provider_authority_device_count": readiness.provider_authority_device_count,
        "trusted_provider_authority_device_count": readiness.trusted_provider_authority_device_count,
        "metadata_ready": readiness.metadata_ready,
        "active_key_id": readiness.active_key_id.as_deref(),
        "app_domain_count": readiness.app_domain_count,
        "enabled_app_domain_count": readiness.enabled_app_domain_count,
        "enabled_sync_content_domain_count": readiness.enabled_sync_content_domain_count,
        "app_domains": profile_sync_app_domains_json(readiness.app_domains.as_slice()),
        "app_domain_readiness": profile_sync_app_domain_readiness_json(
            readiness.app_domain_readiness.as_slice()
        ),
        "storage_provider_count": readiness.storage_provider_count,
        "enabled_storage_provider_count": readiness.enabled_storage_provider_count,
        "retention_capable_provider_count": readiness.retention_capable_provider_count,
        "authorized_retention_provider_count": readiness.authorized_retention_provider_count,
        "authorized_retention_provider_ids": readiness.authorized_retention_provider_ids.as_slice(),
        "storage_providers": profile_sync_storage_providers_json(
            readiness.storage_providers.as_slice()
        ),
        "ready_for_manual_sync": readiness.ready_for_manual_sync,
        "blocked_reason": readiness.blocked_reason.as_deref(),
    })
}

fn profile_sync_app_domain_readiness_json(
    domains: &[slate_storage::AppSyncDomainReadinessRecord],
) -> serde_json::Value {
    serde_json::Value::Array(
        domains
            .iter()
            .map(profile_sync_app_domain_readiness_record_json)
            .collect(),
    )
}

fn profile_sync_app_domain_readiness_record_json(
    domain: &slate_storage::AppSyncDomainReadinessRecord,
) -> serde_json::Value {
    serde_json::json!({
        "domain": domain.domain.as_str(),
        "latest_revision": domain.latest_revision,
    })
}

fn profile_sync_app_domains_json(
    domains: &[slate_storage::AppSyncDomainRecord],
) -> serde_json::Value {
    serde_json::Value::Array(domains.iter().map(profile_sync_app_domain_json).collect())
}

fn profile_sync_app_domain_json(domain: &slate_storage::AppSyncDomainRecord) -> serde_json::Value {
    serde_json::json!({
        "domain": domain.domain.as_str(),
        "schema_version": domain.schema_version,
        "enabled": domain.enabled,
        "privacy_classification": domain.privacy_classification.as_str(),
        "sync_content": domain.sync_content,
    })
}

fn profile_sync_storage_providers_json(
    providers: &[slate_storage::StorageProviderRecord],
) -> serde_json::Value {
    serde_json::Value::Array(
        providers
            .iter()
            .map(profile_sync_storage_provider_json)
            .collect(),
    )
}

fn profile_sync_storage_provider_json(
    provider: &slate_storage::StorageProviderRecord,
) -> serde_json::Value {
    serde_json::json!({
        "provider_id": provider.provider_id.as_str(),
        "provider_kind": provider.provider_kind.as_str(),
        "display_name": provider.display_name.as_str(),
        "endpoint_ref": provider.endpoint_ref.as_deref(),
        "enabled": provider.enabled,
        "discovery": provider.discovery,
        "connectivity": provider.connectivity,
        "object_transfer": provider.object_transfer,
        "availability": provider.availability,
        "mutable_roots": provider.mutable_roots,
        "quota_bytes": provider.quota_bytes,
        "max_retained_objects": provider.max_retained_objects,
        "pinning_policy": provider.pinning_policy.as_deref(),
    })
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

pub(crate) fn apply_synced_chrome_settings_from_database(
    database: &SlateProfileDatabase,
    after_revision: i64,
    limit: u32,
) -> Result<i64, StorageError> {
    let events = database.sync_setting_text_events_after_for_domain(
        DEFAULT_PROFILE_ID,
        SYNC_DOMAIN_SETTINGS,
        after_revision,
        limit,
    )?;
    Ok(apply_synced_chrome_settings_events(
        after_revision,
        events.as_slice(),
    ))
}

pub(crate) fn apply_synced_chrome_settings_events(
    after_revision: i64,
    events: &[SyncSettingTextEvent],
) -> i64 {
    let mut latest_revision = after_revision;
    for event in events {
        latest_revision = latest_revision.max(event.revision.revision);
        apply_synced_chrome_setting_event(event);
    }
    latest_revision
}

fn apply_synced_chrome_setting_event(event: &SyncSettingTextEvent) {
    if event.change.profile != DEFAULT_PROFILE_ID || event.change.domain != SYNC_DOMAIN_SETTINGS {
        return;
    }

    match event.change.entity_key.as_str() {
        CHROME_ELEMENT_ZOOM_SETTING_KEY => match event.change.payload.parse::<f32>() {
            Ok(zoom) => {
                set_current_chrome_element_zoom_setting(zoom);
            }
            Err(error) => warn!("failed to apply synced chrome zoom setting: {error}"),
        },
        key => {
            apply_key_binding_setting(key, event.change.payload.as_str());
        }
    }
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
    json_response(request, 200, settings_state_json(zoom))
}

fn json_response(
    request: &Request,
    status_code: u16,
    body: String,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.status = slate_http_status(status_code);
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(body.into());
    Box::pin(std::future::ready(response))
}

fn profile_sync_json_attachment_response(
    request: &Request,
    filename: &str,
    body: Vec<u8>,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.status = slate_http_status(200);
    response.headers.typed_insert(ContentType::json());
    if let Ok(value) =
        HeaderValue::from_str(format!("attachment; filename=\"{filename}\"").as_str())
    {
        response
            .headers
            .insert(HeaderName::from_static("content-disposition"), value);
    }
    *response.body.lock() = ResponseBody::Done(body);
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

fn profile_sync_key_download_filename_from_url(url: &Url) -> Result<Option<String>, String> {
    if !is_slate_download_request_url(url) {
        return Ok(None);
    }

    let target = url
        .query_pairs()
        .find(|(name, _)| name == "url")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| "missing url query parameter".to_string())?;
    let target_url =
        Url::parse(&target).map_err(|error| format!("invalid download URL: {error}"))?;
    if target_url.scheme() != "slate" {
        return Ok(None);
    }
    if !is_slate_settings_profile_sync_key_export_url(&target_url) {
        return Err(format!(
            "unsupported internal download target: slate://{}{}",
            target_url.host_str().unwrap_or_default(),
            target_url.path()
        ));
    }
    if !profile_sync_download_requested(&target_url) {
        return Err("profile sync key export download flag is required".to_string());
    }

    let filename = url
        .query_pairs()
        .find(|(name, _)| name == "filename")
        .and_then(|(_, value)| non_empty_download_filename(&value))
        .unwrap_or_else(|| profile_sync_key_filename(DEFAULT_PROFILE_ID));
    Ok(Some(filename))
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

fn is_slate_settings_profile_sync_state_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/state"
}

fn is_slate_settings_profile_sync_create_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/create"
}

fn is_slate_settings_profile_sync_check_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/check"
}

fn is_slate_settings_profile_sync_local_provider_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/local-provider"
}

fn is_slate_settings_profile_sync_key_export_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/key/export"
}

fn is_slate_settings_profile_sync_key_import_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/key/import"
}

fn is_slate_settings_profile_sync_handoff_create_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/handoff/create"
}

fn is_slate_settings_profile_sync_handoff_import_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/handoff/import"
}

fn is_slate_settings_profile_sync_run_current_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/run-current"
}

fn is_slate_settings_profile_sync_run_local_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/run-local"
}

fn is_slate_settings_profile_sync_run_local_two_device_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && url.host_str() == Some("settings")
        && url.path().trim_start_matches('/') == "profile-sync/run-local-two-device"
}

fn profile_sync_handoff_target_device_id_from_url(url: &Url) -> Option<String> {
    if !is_slate_settings_profile_sync_handoff_create_url(url) {
        return None;
    }

    url.query_pairs()
        .find(|(name, _)| name == "target_device")
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn profile_sync_handoff_bundle_text_from_url(url: &Url) -> Result<Option<String>, String> {
    if !is_slate_settings_profile_sync_handoff_import_url(url) {
        return Ok(None);
    }

    let bundle_text = url
        .query_pairs()
        .find(|(name, _)| name == "handoff")
        .map(|(_, value)| value.into_owned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(bundle_text) = bundle_text else {
        return Ok(None);
    };
    if bundle_text.len() > PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES {
        return Err(format!(
            "profile sync enrollment file is too large; maximum is {PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES} bytes"
        ));
    }

    Ok(Some(bundle_text))
}

fn profile_sync_key_text_from_url(url: &Url) -> Result<Option<String>, String> {
    if !is_slate_settings_profile_sync_key_import_url(url) {
        return Ok(None);
    }

    let key_text = url
        .query_pairs()
        .find(|(name, _)| name == "key")
        .map(|(_, value)| value.into_owned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(key_text) = key_text else {
        return Ok(None);
    };
    if key_text.len() > PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES {
        return Err(format!(
            "profile sync enrollment key is too large; maximum is {PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES} bytes"
        ));
    }

    Ok(Some(key_text))
}

fn profile_sync_download_requested(url: &Url) -> bool {
    url.query_pairs()
        .any(|(name, value)| name == "download" && matches!(value.as_ref(), "1" | "true" | "yes"))
}

fn profile_sync_key_filename(profile: &str) -> String {
    let safe_profile = sanitized_profile_sync_filename_token(profile);
    format!("slate-profile-enrollment-key-{safe_profile}.json")
}

fn profile_sync_handoff_filename(target_device_id: &str) -> String {
    format!("slate-profile-enrollment-{target_device_id}.json")
}

fn sanitized_profile_sync_filename_token(value: &str) -> String {
    let token = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let token = token.trim_matches('-');
    if token.is_empty() {
        "default".to_string()
    } else {
        token.to_string()
    }
}

fn unix_time_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        CHROME_ELEMENT_ZOOM_SETTING_MAX, CHROME_ELEMENT_ZOOM_SETTING_MIN,
        chrome_element_zoom_setting_from_url, download_request_from_url, is_slate_blank_url,
        is_slate_calendar_url, is_slate_chat_url, is_slate_contacts_url,
        is_slate_download_request_url, is_slate_downloads_state_url, is_slate_downloads_url,
        is_slate_files_url, is_slate_home_url, is_slate_settings_apply_url,
        is_slate_settings_preview_url, is_slate_settings_profile_sync_check_url,
        is_slate_settings_profile_sync_create_url,
        is_slate_settings_profile_sync_handoff_create_url,
        is_slate_settings_profile_sync_handoff_import_url,
        is_slate_settings_profile_sync_key_export_url,
        is_slate_settings_profile_sync_key_import_url,
        is_slate_settings_profile_sync_local_provider_url,
        is_slate_settings_profile_sync_run_current_url, is_slate_settings_profile_sync_state_url,
        is_slate_settings_save_url, is_slate_settings_url, is_slate_web_url,
        profile_sync_download_requested, profile_sync_handoff_bundle_text_from_url,
        profile_sync_handoff_target_device_id_from_url,
        profile_sync_key_download_filename_from_url, profile_sync_key_filename,
        profile_sync_key_text_from_url, slate_download_error_html,
    };
    use crate::desktop::key_bindings::{
        SlateKeyBindings, current_key_bindings_json_value, set_current_key_bindings,
    };
    use slate_broadwebd::FetchPurpose;
    use slate_broadwebd::TemporaryDownloadRecord;
    use slate_storage::PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES;
    use slate_storage::{
        DEFAULT_PROFILE_ID, IncomingSyncSettingText, SYNC_DOMAIN_CALENDAR, SYNC_DOMAIN_SETTINGS,
        SlateProfileDatabase, SlateSyncSecret,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use url::Url;

    fn unique_database_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "slate-protocol-settings-{name}-{}-{}.db",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

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
    fn slate_download_request_recognizes_profile_sync_key_export() {
        let filename = profile_sync_key_download_filename_from_url(
            &Url::parse(
                "slate://download?url=slate%3A%2F%2Fsettings%2Fprofile-sync%2Fkey%2Fexport%3Fdownload%3D1&filename=key.json",
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(filename.as_deref(), Some("key.json"));
        let default_filename = profile_sync_key_download_filename_from_url(
            &Url::parse(
                "slate://download?url=slate%3A%2F%2Fsettings%2Fprofile-sync%2Fkey%2Fexport%3Fdownload%3D1",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            default_filename.as_deref(),
            Some("slate-profile-enrollment-key-default.json")
        );
        let click_time_filename = profile_sync_key_download_filename_from_url(
            &Url::parse(
                "slate://download?url=slate%3A%2F%2Fsettings%2Fprofile-sync%2Fkey%2Fexport%3Fnonce%3D123%26download%3D1&filename=slate-profile-enrollment-key-default.json",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            click_time_filename.as_deref(),
            Some("slate-profile-enrollment-key-default.json")
        );
        assert_eq!(
            profile_sync_key_download_filename_from_url(
                &Url::parse(
                    "slate://download?url=https%3A%2F%2Fexample.com%2Freleases%2Fslate.tar.gz",
                )
                .unwrap(),
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn slate_download_request_rejects_other_internal_targets() {
        let missing_flag = profile_sync_key_download_filename_from_url(
            &Url::parse(
                "slate://download?url=slate%3A%2F%2Fsettings%2Fprofile-sync%2Fkey%2Fexport",
            )
            .unwrap(),
        )
        .unwrap_err();
        let wrong_page = profile_sync_key_download_filename_from_url(
            &Url::parse("slate://download?url=slate%3A%2F%2Fsettings").unwrap(),
        )
        .unwrap_err();

        assert_eq!(
            missing_flag,
            "profile sync key export download flag is required"
        );
        assert!(wrong_page.contains("unsupported internal download target"));
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
    fn slate_settings_profile_sync_action_urls_are_distinct() {
        let state = Url::parse("slate://settings/profile-sync/state").unwrap();
        let create = Url::parse("slate://settings/profile-sync/create").unwrap();
        let check = Url::parse("slate://settings/profile-sync/check").unwrap();
        let local_provider = Url::parse("slate://settings/profile-sync/local-provider").unwrap();
        let key_export = Url::parse("slate://settings/profile-sync/key/export?download=1").unwrap();
        let key_import = Url::parse("slate://settings/profile-sync/key/import?key=%7B%7D").unwrap();
        let handoff_create =
            Url::parse("slate://settings/profile-sync/handoff/create?target_device=device-b")
                .unwrap();
        let handoff_import =
            Url::parse("slate://settings/profile-sync/handoff/import?handoff=%7B%7D").unwrap();
        let run_current = Url::parse("slate://settings/profile-sync/run-current").unwrap();
        let run_local = Url::parse("slate://settings/profile-sync/run-local").unwrap();
        let run_two_device =
            Url::parse("slate://settings/profile-sync/run-local-two-device").unwrap();

        assert!(is_slate_settings_profile_sync_state_url(&state));
        assert!(is_slate_settings_profile_sync_create_url(&create));
        assert!(is_slate_settings_profile_sync_check_url(&check));
        assert!(is_slate_settings_profile_sync_local_provider_url(
            &local_provider
        ));
        assert!(is_slate_settings_profile_sync_key_export_url(&key_export));
        assert!(is_slate_settings_profile_sync_key_import_url(&key_import));
        assert!(is_slate_settings_profile_sync_handoff_create_url(
            &handoff_create
        ));
        assert!(is_slate_settings_profile_sync_handoff_import_url(
            &handoff_import
        ));
        assert!(is_slate_settings_profile_sync_run_current_url(&run_current));
        assert!(is_slate_settings_profile_sync_run_local_url(&run_local));
        assert!(is_slate_settings_profile_sync_run_local_two_device_url(
            &run_two_device
        ));
        assert!(!is_slate_settings_profile_sync_create_url(&state));
        assert!(!is_slate_settings_profile_sync_check_url(&create));
        assert!(!is_slate_settings_profile_sync_local_provider_url(&check));
        assert!(!is_slate_settings_profile_sync_key_export_url(&key_import));
        assert!(!is_slate_settings_profile_sync_key_import_url(&key_export));
        assert!(!is_slate_settings_profile_sync_handoff_create_url(
            &local_provider
        ));
        assert!(!is_slate_settings_profile_sync_handoff_import_url(
            &handoff_create
        ));
        assert!(!is_slate_settings_profile_sync_run_local_url(
            &local_provider
        ));
        assert!(!is_slate_settings_profile_sync_run_current_url(&run_local));
        assert!(!is_slate_settings_profile_sync_run_local_two_device_url(
            &run_local
        ));
        assert_eq!(
            profile_sync_handoff_target_device_id_from_url(&handoff_create).as_deref(),
            Some("device-b")
        );
        assert_eq!(
            profile_sync_handoff_bundle_text_from_url(&handoff_import)
                .unwrap()
                .as_deref(),
            Some("{}")
        );
        assert_eq!(
            profile_sync_key_text_from_url(&key_import)
                .unwrap()
                .as_deref(),
            Some("{}")
        );
        assert!(profile_sync_download_requested(&key_export));
        assert_eq!(
            profile_sync_key_filename("default"),
            "slate-profile-enrollment-key-default.json"
        );
        assert_eq!(
            profile_sync_key_filename("../work profile"),
            "slate-profile-enrollment-key-work-profile.json"
        );
        assert_eq!(
            profile_sync_handoff_target_device_id_from_url(
                &Url::parse("slate://settings/profile-sync/handoff/create?target_device=").unwrap()
            ),
            None
        );
        assert_eq!(
            profile_sync_handoff_bundle_text_from_url(&Url::parse("slate://settings").unwrap())
                .unwrap(),
            None
        );
        assert_eq!(
            profile_sync_key_text_from_url(&Url::parse("slate://settings").unwrap()).unwrap(),
            None
        );
        let oversized_handoff = Url::parse(&format!(
            "slate://settings/profile-sync/handoff/import?handoff={}",
            "a".repeat(PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES + 1)
        ))
        .unwrap();
        assert!(
            profile_sync_handoff_bundle_text_from_url(&oversized_handoff)
                .unwrap_err()
                .contains("too large")
        );
        let oversized_key = Url::parse(&format!(
            "slate://settings/profile-sync/key/import?key={}",
            "a".repeat(PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES + 1)
        ))
        .unwrap();
        assert!(
            profile_sync_key_text_from_url(&oversized_key)
                .unwrap_err()
                .contains("too large")
        );
    }

    #[test]
    fn profile_sync_preview_state_creates_downloadable_key_for_first_use() {
        let mut state = super::ProfileSyncPreviewState::default();
        let (sync_secret, export) = state
            .create_secret_export_for_unenrolled_profile(DEFAULT_PROFILE_ID, 123)
            .expect("create first-use enrollment key");

        assert_eq!(export.profile, DEFAULT_PROFILE_ID);
        assert_eq!(export.created_at, 123);
        assert_eq!(state.active_export.as_ref(), Some(&export));
        let decoded_secret = SlateSyncSecret::from_export_for_profile(&export, DEFAULT_PROFILE_ID)
            .expect("decode exported sync secret");
        assert_eq!(decoded_secret, sync_secret);

        let ordinary_state: serde_json::Value =
            serde_json::from_str(&state.to_json(None)).expect("ordinary state json");
        assert!(
            !ordinary_state
                .as_object()
                .unwrap()
                .contains_key("key_export_text")
        );

        let export_state: serde_json::Value =
            serde_json::from_str(&state.to_json_with_key_export(None)).expect("export state json");
        assert!(
            export_state["key_export_text"]
                .as_str()
                .expect("key export text")
                .contains("\"secret\"")
        );
    }

    #[test]
    fn profile_sync_preview_state_exports_only_explicit_handoff_file() {
        let mut source = super::ProfileSyncPreviewState::default();
        let sync_secret = source.create_secret(DEFAULT_PROFILE_ID, 123).unwrap();
        let handoff_bundle = SlateProfileDatabase::profile_sync_secret_handoff_bundle_from_secret(
            DEFAULT_PROFILE_ID,
            &sync_secret,
            "device-b",
        )
        .unwrap();
        source.mark_secret_handoff_bundle(&handoff_bundle);
        let source_json: serde_json::Value = serde_json::from_str(&source.to_json(None)).unwrap();
        let source_object = source_json.as_object().unwrap();

        assert_eq!(source_json["profile"], DEFAULT_PROFILE_ID);
        assert_eq!(source_json["status"], "ready");
        assert_eq!(source_json["has_secret"], true);
        assert!(!source_object.contains_key("export_filename"));
        assert!(!source_object.contains_key("export_text"));
        assert!(!source_object.contains_key("device_request_export_filename"));
        assert!(!source_object.contains_key("device_request_export_text"));
        assert!(!source_object.contains_key("device_request_device_id"));
        assert!(!source_object.contains_key("enrollment_export_filename"));
        assert!(!source_object.contains_key("enrollment_export_text"));
        assert!(!source_object.contains_key("enrollment_target_device_id"));
        assert!(!source_object.contains_key("enrollment_signed_record_count"));
        assert!(!source_object.contains_key("key_export_text"));
        assert_eq!(
            source_json["key_export_filename"],
            "slate-profile-enrollment-key-default.json"
        );
        assert!(!source_object.contains_key("handoff_export_text"));
        assert_eq!(
            source_json["handoff_export_filename"],
            "slate-profile-enrollment-device-b.json"
        );
        assert_eq!(source_json["handoff_target_device_id"], "device-b");

        let key_json: serde_json::Value =
            serde_json::from_str(&source.to_json_with_key_export(None)).unwrap();
        let key_object = key_json.as_object().unwrap();
        let key_export_text = key_json["key_export_text"].as_str().unwrap();
        assert!(key_export_text.contains("\"secret\""));
        assert!(key_export_text.contains("\"profile\":\"default\""));
        assert!(!key_object.contains_key("handoff_export_text"));

        let handoff_json: serde_json::Value =
            serde_json::from_str(&source.to_json_with_handoff_export(None)).unwrap();
        let handoff_object = handoff_json.as_object().unwrap();
        let handoff_export_text = handoff_json["handoff_export_text"].as_str().unwrap();
        assert!(handoff_export_text.contains("device-b"));
        assert!(handoff_export_text.contains("sync_secret_export"));
        assert!(!handoff_object.contains_key("export_text"));
        assert!(!handoff_object.contains_key("device_request_export_text"));
        assert!(!handoff_object.contains_key("enrollment_export_text"));

        let export_text = source
            .active_export
            .as_ref()
            .and_then(|export| export.to_bytes().ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap();
        let mut destination = super::ProfileSyncPreviewState::default();
        destination
            .import_secret(DEFAULT_PROFILE_ID, export_text.as_str())
            .unwrap();
        let destination_json: serde_json::Value =
            serde_json::from_str(&destination.to_json(None)).unwrap();

        assert_eq!(destination_json["status"], "ready");
        assert_eq!(destination_json["last_error"], serde_json::Value::Null);

        let error = destination
            .import_secret("work", export_text.as_str())
            .unwrap_err();
        assert!(error.to_string().contains("unexpected sync object profile"));
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
    fn synced_chrome_settings_feed_updates_runtime_state() {
        let path = unique_database_path("runtime-feed");
        let database = SlateProfileDatabase::open_resolved(path.clone()).unwrap();
        let baseline_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();

        super::set_current_chrome_element_zoom_setting(super::CHROME_ELEMENT_ZOOM_SETTING_DEFAULT);
        set_current_key_bindings(SlateKeyBindings::default());
        let zoom_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "chrome.zoom",
                "1.05",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "keybindings.next_tab",
                "Alt+ArrowRight",
            )
            .unwrap();

        let revision =
            super::apply_synced_chrome_settings_from_database(&database, baseline_revision, 64)
                .unwrap();
        assert!(revision > baseline_revision);
        assert_eq!(super::current_chrome_element_zoom_setting(), 1.05);
        let key_bindings = current_key_bindings_json_value();
        let next_tab = key_bindings
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "next_tab")
            .expect("next_tab binding");
        assert_eq!(next_tab["value"], "Alt+ArrowRight");

        let losing_zoom = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "chrome.zoom",
                "0.80",
                "device-b",
                1,
                zoom_change.logical_clock - 1,
            ))
            .unwrap();
        assert_eq!(losing_zoom.applied_at, None);
        let calendar_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        assert!(calendar_change.id > zoom_change.id);
        let unchanged_revision =
            super::apply_synced_chrome_settings_from_database(&database, revision, 64).unwrap();
        assert_eq!(unchanged_revision, revision);
        assert_eq!(super::current_chrome_element_zoom_setting(), 1.05);

        drop(database);
        let _ = std::fs::remove_file(path);
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
        assert!(files_page.contains("File browser mockup"));
        assert!(files_page.contains("Storage Settings"));
        assert!(!files_page.contains("IPFS Cache"));
        assert!(!files_page.contains("Saved CARs"));
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
        assert!(settings_page.contains("Profile Sync"));
        assert!(settings_page.contains("id=\"profile-sync-details\""));
        assert!(settings_page.contains("id=\"profile-sync-enrollment-key-file\""));
        assert!(settings_page.contains("id=\"profile-sync-key-download\""));
        assert!(settings_page.contains("id=\"profile-sync-key-import\""));
        assert!(settings_page.contains("setProfileSyncEnrollmentBusy"));
        assert!(settings_page.contains("href=\"slate://download?url=slate%3A%2F%2Fsettings%2Fprofile-sync%2Fkey%2Fexport%3Fdownload%3D1\""));
        assert!(settings_page.contains("Saving ${enrollmentKeyFilename(currentProfileSyncState)}"));
        assert!(settings_page.contains("Importing enrollment key"));
        assert!(settings_page.contains("created on download"));
        assert!(settings_page.contains("import needed"));
        assert!(settings_page.contains("PROFILE_SYNC_ENROLLMENT_KEY_MAX_BYTES"));
        assert!(settings_page.contains("profileSyncBoundedFileText"));
        assert!(settings_page.contains("Download enrollment key"));
        assert!(settings_page.contains("Import enrollment key"));
        assert!(settings_page.contains("updateProfileSyncDownloadHref"));
        assert!(settings_page.contains("slateDownloadUrl(targetUrl"));
        assert!(settings_page.contains("download: \"1\""));
        assert!(settings_page.contains("chooseProfileSyncEnrollmentKey"));
        assert!(settings_page.contains("profileSyncEnrollmentKeyFile.click()"));
        assert!(settings_page.contains("Enrollment key"));
        assert!(!settings_page.contains("Local diagnostics"));
        assert!(!settings_page.contains("Preparing enrollment key"));
        assert!(!settings_page.contains("ensureProfileSyncKeyForDownload"));
        assert!(!settings_page.contains("triggerProfileSyncEnrollmentKeyDownload"));
        assert!(!settings_page.contains("id=\"profile-sync-create\""));
        assert!(!settings_page.contains("Create local key"));
        assert!(!settings_page.contains("id=\"profile-sync-provider\""));
        assert!(!settings_page.contains("id=\"profile-sync-check\""));
        assert!(!settings_page.contains("id=\"profile-sync-run-current\""));
        assert!(!settings_page.contains("id=\"profile-sync-run-local\""));
        assert!(!settings_page.contains("id=\"profile-sync-run-local-two-device\""));
        assert!(settings_page.contains("profileSyncUrl(\"key/export\""));
        assert!(settings_page.contains("fetchProfileSyncState(\"key/import\""));
        assert!(!settings_page.contains("key_export_text"));
        assert!(!settings_page.contains("window.URL.createObjectURL"));
        assert!(!settings_page.contains("data:application/json"));
        assert!(!settings_page.contains("id=\"profile-sync-handoff-device\""));
        assert!(!settings_page.contains("profileSyncHandoffDevice"));
        assert!(!settings_page.contains("id=\"profile-sync-handoff-file\""));
        assert!(!settings_page.contains("id=\"profile-sync-handoff-download\""));
        assert!(!settings_page.contains("id=\"profile-sync-handoff-import\""));
        assert!(!settings_page.contains("PROFILE_SYNC_HANDOFF_FILE_MAX_BYTES"));
        assert!(!settings_page.contains("chooseProfileSyncHandoffBundle"));
        assert!(!settings_page.contains("profileSyncHandoffFile"));
        assert!(!settings_page.contains("id=\"profile-sync-secret\""));
        assert!(!settings_page.contains("id=\"profile-sync-secret-file\""));
        assert!(!settings_page.contains("id=\"profile-sync-download\""));
        assert!(!settings_page.contains("id=\"profile-sync-import\""));
        assert!(!settings_page.contains("id=\"profile-sync-device-request-file\""));
        assert!(!settings_page.contains("id=\"profile-sync-device-request\""));
        assert!(!settings_page.contains("id=\"profile-sync-enrollment-file\""));
        assert!(!settings_page.contains("id=\"profile-sync-enrollment\""));
        assert!(!settings_page.contains("id=\"profile-sync-handoff-create\""));
        assert!(!settings_page.contains("id=\"profile-sync-handoff\""));
        assert!(settings_page.contains("Local sync"));
        assert!(settings_page.contains("profileSyncLocalStatus"));
        assert!(!settings_page.contains("Enabled domains"));
        assert!(!settings_page.contains("Last check"));
        assert!(!settings_page.contains("profileSyncEnabledAppDomainStatus"));
        assert!(!settings_page.contains("profileSyncAppDomainHeadStatus"));
        assert!(!settings_page.contains("profileSyncContentAppDomainStatus"));
        assert!(!settings_page.contains("profileSyncActiveProviderStatus"));
        assert!(!settings_page.contains("profileSyncAuthorizedProviderStatus"));
        assert!(!settings_page.contains("profileSyncDiscoveryStatus"));
        assert!(!settings_page.contains("profileSyncHealthStatus"));
        assert!(!settings_page.contains("profileSyncIssueStatus"));
        assert!(!settings_page.contains("Issue details"));
        assert!(!settings_page.contains("profileSyncIssueDetails"));
        assert!(!settings_page.contains("profileSyncLatestIssueRun"));
        assert!(!settings_page.contains("candidates.push(state.last_two_device_trial)"));
        assert!(!settings_page.contains("discovery_rejections"));
        assert!(!settings_page.contains("retention_provider_selection_issues"));
        assert!(!settings_page.contains("stored_provider_metadata_issues"));
        assert!(!settings_page.contains("profileSyncIssueLabel"));
        assert!(settings_page.contains("slate://settings/profile-sync/"));
        assert!(settings_page.contains("slate-profile-enrollment"));
        assert!(!settings_page.contains("slate-sync-secret.json"));
        assert!(!settings_page.contains("replaceState"));
        assert!(!settings_page.contains("type=\"range\""));
    }
}
