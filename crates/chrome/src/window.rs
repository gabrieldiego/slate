/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::AtomicU64;

use euclid::Scale;
use log::warn;
use servo::{
    AuthenticationRequest, BluetoothDeviceSelectionRequest, ConsoleLogLevel, Cursor,
    DeviceIndependentIntRect, DeviceIndependentPixel, DeviceIntPoint, DeviceIntSize, DevicePixel,
    EmbedderControl, EmbedderControlId, InputEventId, InputEventResult, MediaSessionEvent,
    PermissionRequest, RenderingContext, ScreenGeometry, WebView, WebViewBuilder, WebViewId,
};
use url::Url;

use crate::parser::location_bar_input_to_url;
use crate::running_app_state::{RunningAppState, UserInterfaceCommand, WebViewCollection};

// This should vary by zoom level and maybe actual text size (focused or under cursor)
#[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
pub(crate) const LINE_HEIGHT: f32 = 76.0;
#[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
pub(crate) const LINE_WIDTH: f32 = 76.0;

/// <https://github.com/web-platform-tests/wpt/blob/9320b1f724632c52929a3fdb11bdaf65eafc7611/webdriver/tests/classic/set_window_rect/set.py#L287-L290>
/// "A window size of 10x10px shouldn't be supported by any browser."
#[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
pub(crate) const MIN_WINDOW_INNER_SIZE: DeviceIntSize = DeviceIntSize::new(100, 100);

static SERVOSHELL_WINDOW_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) struct ServoShellWindowId(u64);

impl From<u64> for ServoShellWindowId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl ServoShellWindowId {
    #[cfg_attr(not(any(target_os = "android", target_env = "ohos")), expect(unused))]
    pub(crate) fn next() -> ServoShellWindowId {
        ServoShellWindowId(SERVOSHELL_WINDOW_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

pub(crate) struct ServoShellWindow {
    /// The [`WebView`]s that have been added to this window.
    pub(crate) webview_collection: RefCell<WebViewCollection>,
    /// A handle to the [`PlatformWindow`] that servoshell is rendering in.
    platform_window: Rc<dyn PlatformWindow>,
    /// Whether or not this window should be closed at the end of the spin of the next event loop.
    close_scheduled: Cell<bool>,
    /// Whether or not the application interface needs to be updated.
    needs_update: Cell<bool>,
    /// Whether or not Servo needs to repaint its display. Currently this is global
    /// because every `WebView` shares a `RenderingContext`.
    needs_repaint: Cell<bool>,
    /// List of webviews that have favicon textures which are not yet uploaded
    /// to the GPU by egui.
    pending_favicon_loads: RefCell<Vec<WebViewId>>,
    /// Pending [`UserInterfaceCommand`] that have yet to be processed by the main loop.
    pending_commands: RefCell<Vec<UserInterfaceCommand>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReusableInternalPage {
    Home,
    Web,
    Downloads,
    Calendar,
    Chat,
    Settings,
}

impl ReusableInternalPage {
    fn closes_duplicates(self) -> bool {
        true
    }

    fn existing_target_index(self, matching_count: usize) -> Option<usize> {
        if matching_count == 0 {
            return None;
        }

        match self {
            Self::Home
            | Self::Web
            | Self::Downloads
            | Self::Calendar
            | Self::Chat
            | Self::Settings => Some(matching_count - 1),
        }
    }
}

fn reusable_internal_page(url: &Url) -> Option<ReusableInternalPage> {
    if url.scheme() != "slate" {
        return None;
    }

    let path = url.path().trim_matches('/');
    match (url.host_str(), path) {
        (Some("home"), "") | (None, "home") => Some(ReusableInternalPage::Home),
        (Some("web"), "") | (None, "web") => Some(ReusableInternalPage::Web),
        (Some("downloads"), "") | (None, "downloads") => Some(ReusableInternalPage::Downloads),
        (Some("calendar"), "") | (None, "calendar") => Some(ReusableInternalPage::Calendar),
        (Some("chat"), "") | (None, "chat") | (Some("messages"), "") | (None, "messages") => {
            Some(ReusableInternalPage::Chat)
        }
        (Some("settings"), "") | (None, "settings") => Some(ReusableInternalPage::Settings),
        _ => None,
    }
}

impl ServoShellWindow {
    pub(crate) fn new(platform_window: Rc<dyn PlatformWindow>) -> Self {
        Self {
            webview_collection: Default::default(),
            platform_window,
            close_scheduled: Default::default(),
            needs_update: Default::default(),
            needs_repaint: Default::default(),
            pending_favicon_loads: Default::default(),
            pending_commands: Default::default(),
        }
    }

    pub(crate) fn id(&self) -> ServoShellWindowId {
        self.platform_window().id()
    }

    /// Must be called *after* `self` is in `state.windows`, otherwise it will panic.
    pub(crate) fn create_and_activate_toplevel_webview(
        self: &Rc<Self>,
        state: Rc<RunningAppState>,
        url: Url,
    ) -> WebView {
        let webview = self.create_toplevel_webview(state, url);
        self.activate_webview(webview.id());
        webview
    }

    /// Must be called *after* `self` is in `state.windows`, otherwise it will panic.
    #[servo::servo_tracing::instrument(skip(self, state))]
    pub(crate) fn create_toplevel_webview(
        self: &Rc<Self>,
        state: Rc<RunningAppState>,
        url: Url,
    ) -> WebView {
        let webview_builder =
            WebViewBuilder::new(state.servo(), self.platform_window.rendering_context())
                .url(url)
                .hidpi_scale_factor(self.platform_window.hidpi_scale_factor())
                .user_content_manager(state.user_content_manager.clone())
                .delegate(state.clone());

        #[cfg(all(
            feature = "gamepad",
            not(any(target_os = "android", target_env = "ohos"))
        ))]
        let webview_builder = {
            let mut webview_builder = webview_builder;
            if let Some(gamepad_delegate) = state.gamepad_delegate() {
                webview_builder = webview_builder.gamepad_delegate(gamepad_delegate);
            }
            webview_builder
        };

        let webview = webview_builder.build();
        webview.notify_theme_change(self.platform_window.theme());
        self.add_webview(webview.clone());

        // If `self` is not in `state.windows`, our notify_accessibility_tree_update() will panic.
        if state.accessibility_active() {
            // Activate accessibility in the WebView.
            // There are two sites like this; this is the WebView creation site.
            webview.set_accessibility_active(true);
        }
        webview
    }

    /// Repaint the focused [`WebView`].
    pub(crate) fn repaint_webviews(&self) {
        let Some(webview) = self.active_webview() else {
            return;
        };

        self.platform_window()
            .rendering_context()
            .make_current()
            .expect("Could not make PlatformWindow RenderingContext current");
        webview.paint();
        self.platform_window().rendering_context().present();
    }

    /// Whether or not this [`ServoShellWindow`] has any [`WebView`]s.
    pub(crate) fn should_close(&self) -> bool {
        self.webview_collection.borrow().is_empty() || self.close_scheduled.get()
    }

    pub(crate) fn webview_by_id(&self, id: WebViewId) -> Option<WebView> {
        self.webview_collection.borrow().get(id).cloned()
    }

    pub(crate) fn set_needs_update(&self) {
        self.needs_update.set(true);
    }

    pub(crate) fn set_needs_repaint(&self) {
        self.needs_repaint.set(true)
    }

    #[cfg_attr(target_os = "android", expect(dead_code))]
    pub(crate) fn schedule_close(&self) {
        self.close_scheduled.set(true)
    }

    pub(crate) fn platform_window(&self) -> Rc<dyn PlatformWindow> {
        self.platform_window.clone()
    }

    pub(crate) fn focus(&self) {
        self.platform_window.focus()
    }

    pub(crate) fn add_webview(&self, webview: WebView) {
        self.webview_collection.borrow_mut().add(webview);
        self.set_needs_update();
        self.set_needs_repaint();
    }

    pub(crate) fn webview_ids(&self) -> Vec<WebViewId> {
        self.webview_collection.borrow().creation_order.clone()
    }

    /// Returns all [`WebView`]s in creation order.
    pub(crate) fn webviews(&self) -> Vec<(WebViewId, WebView)> {
        self.webview_collection
            .borrow()
            .all_in_creation_order()
            .map(|(id, webview)| (id, webview.clone()))
            .collect()
    }

    pub(crate) fn activate_webview(&self, webview_id: WebViewId) {
        self.webview_collection
            .borrow_mut()
            .activate_webview(webview_id);
        self.set_needs_update();
    }

    #[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
    pub(crate) fn activate_webview_by_index(&self, index_to_activate: usize) {
        self.webview_collection
            .borrow_mut()
            .activate_webview_by_index(index_to_activate);
        self.set_needs_update();
    }

    #[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
    pub(crate) fn get_active_webview_index(&self) -> Option<usize> {
        let active_id = self.webview_collection.borrow().active_id()?;
        self.webviews()
            .iter()
            .position(|webview| webview.0 == active_id)
    }

    pub(crate) fn update_and_request_repaint_if_necessary(&self, state: &RunningAppState) {
        let updated_user_interface = self.needs_update.take()
            && self
                .platform_window
                .update_user_interface_state(state, self);

        // Delegate handlers may have asked us to present or update painted WebView contents.
        // Currently, egui-file-dialog dialogs need to be constantly redrawn or animations aren't fluid.
        let needs_repaint = self.needs_repaint.take();
        if updated_user_interface || needs_repaint {
            self.platform_window.request_repaint(self);
        }
    }

    /// Close the given [`WebView`] via its [`WebViewId`].
    ///
    /// Note: This can happen because we can trigger a close with a UI action and then get
    /// the close notification via the [`WebViewDelegate`] later.
    pub(crate) fn close_webview(&self, webview_id: WebViewId) {
        let mut webview_collection = self.webview_collection.borrow_mut();
        if webview_collection.remove(webview_id).is_none() {
            return;
        }
        self.platform_window
            .dismiss_embedder_controls_for_webview(webview_id);

        self.set_needs_update();
        self.set_needs_repaint();
    }

    pub(crate) fn notify_favicon_changed(&self, webview: WebView) {
        self.pending_favicon_loads.borrow_mut().push(webview.id());
        self.set_needs_repaint();
    }

    #[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
    pub(crate) fn hidpi_scale_factor_changed(&self) {
        let new_scale_factor = self.platform_window.hidpi_scale_factor();
        for webview in self.webview_collection.borrow().values() {
            webview.set_hidpi_scale_factor(new_scale_factor);
        }
    }

    pub(crate) fn active_webview(&self) -> Option<WebView> {
        self.webview_collection.borrow().active().cloned()
    }

    #[cfg_attr(
        not(any(target_os = "android", target_env = "ohos")),
        expect(dead_code)
    )]
    pub(crate) fn active_or_newest_webview(&self) -> Option<WebView> {
        let webview_collection = self.webview_collection.borrow();
        webview_collection
            .active()
            .or(webview_collection.newest())
            .cloned()
    }

    fn open_reusable_internal_page(self: &Rc<Self>, state: &Rc<RunningAppState>, url: Url) -> bool {
        let Some(page) = reusable_internal_page(&url) else {
            return false;
        };

        let matching_ids = self
            .webviews()
            .into_iter()
            .filter_map(|(id, webview)| {
                let url = webview.url()?;
                (reusable_internal_page(&url) == Some(page)).then_some(id)
            })
            .collect::<Vec<_>>();

        if let Some(target_id) = page
            .existing_target_index(matching_ids.len())
            .and_then(|index| matching_ids.get(index).copied())
        {
            if page.closes_duplicates() {
                for duplicate_id in matching_ids
                    .into_iter()
                    .filter(|webview_id| *webview_id != target_id)
                {
                    self.close_webview(duplicate_id);
                }
            }
            self.activate_webview(target_id);
        } else {
            self.create_and_activate_toplevel_webview(state.clone(), url);
        }

        true
    }

    /// Return a list of all webviews that have favicons that have not yet been loaded by egui.
    #[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
    pub(crate) fn take_pending_favicon_loads(&self) -> Vec<WebViewId> {
        std::mem::take(&mut *self.pending_favicon_loads.borrow_mut())
    }

    pub(crate) fn show_embedder_control(
        &self,
        webview: WebView,
        embedder_control: EmbedderControl,
    ) {
        self.platform_window
            .show_embedder_control(webview.id(), embedder_control);
        self.set_needs_update();
        self.set_needs_repaint();
    }

    pub(crate) fn hide_embedder_control(
        &self,
        webview: WebView,
        embedder_control: EmbedderControlId,
    ) {
        self.platform_window
            .hide_embedder_control(webview.id(), embedder_control);
        self.set_needs_update();
        self.set_needs_repaint();
    }

    pub(crate) fn queue_user_interface_command(&self, command: UserInterfaceCommand) {
        self.pending_commands.borrow_mut().push(command)
    }

    /// Takes any events generated during UI updates and performs their actions.
    pub(crate) fn handle_interface_commands(
        self: &Rc<Self>,
        state: &Rc<RunningAppState>,
        create_platform_window: Option<&dyn Fn(Url) -> Rc<dyn PlatformWindow>>,
    ) {
        let commands = std::mem::take(&mut *self.pending_commands.borrow_mut());
        for event in commands {
            match event {
                UserInterfaceCommand::Go(location) => {
                    self.set_needs_update();
                    let Some(url) = location_bar_input_to_url(
                        &location.clone(),
                        &state.servoshell_preferences.searchpage,
                    ) else {
                        warn!("failed to parse location");
                        break;
                    };
                    let url = url.into_url();
                    if self.open_reusable_internal_page(state, url.clone()) {
                        continue;
                    }
                    if let Some(active_webview) = self.active_webview() {
                        active_webview.load(url);
                    }
                }
                UserInterfaceCommand::Back => {
                    if let Some(active_webview) = self.active_webview() {
                        active_webview.go_back(1);
                    }
                }
                UserInterfaceCommand::Forward => {
                    if let Some(active_webview) = self.active_webview() {
                        active_webview.go_forward(1);
                    }
                }
                UserInterfaceCommand::Reload => {
                    self.set_needs_update();
                    if let Some(active_webview) = self.active_webview() {
                        active_webview.reload();
                    }
                }
                UserInterfaceCommand::ReloadAll => {
                    for window in state.windows().values() {
                        window.set_needs_update();
                        for (_, webview) in window.webviews() {
                            webview.reload();
                        }
                    }
                }
                UserInterfaceCommand::NewWebView => {
                    self.set_needs_update();
                    let url = Url::parse("slate://blank").expect("Should always be able to parse");
                    self.create_and_activate_toplevel_webview(state.clone(), url);
                }
                UserInterfaceCommand::CloseWebView(id) => {
                    self.set_needs_update();
                    self.close_webview(id);
                }
                UserInterfaceCommand::NewWindow => {
                    if let Some(create_platform_window) = create_platform_window {
                        let url = Url::parse("slate://home").unwrap();
                        let platform_window = create_platform_window(url.clone());
                        state.open_window(platform_window, url);
                    }
                }
            }
        }
    }
}

/// A `PlatformWindow` abstracts away the differents kinds of platform windows that might
/// be used in a servoshell execution. This currently includes headed (winit) and headless
/// windows.
pub(crate) trait PlatformWindow {
    fn id(&self) -> ServoShellWindowId;
    fn screen_geometry(&self) -> ScreenGeometry;
    #[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
    fn device_hidpi_scale_factor(&self) -> Scale<f32, DeviceIndependentPixel, DevicePixel>;
    fn hidpi_scale_factor(&self) -> Scale<f32, DeviceIndependentPixel, DevicePixel>;
    #[cfg_attr(any(target_os = "android", target_env = "ohos"), expect(dead_code))]
    fn get_fullscreen(&self) -> bool;
    /// Inform the `Window` that the state of a `WebView` has changed and that it should
    /// do an incremental update of user interface state. Returns `true` if the user
    /// interface actually changed and a rebuild  and repaint is needed, `false` otherwise.
    fn update_user_interface_state(&self, _: &RunningAppState, _: &ServoShellWindow) -> bool {
        false
    }
    /// Request that the window redraw itself. It is up to the window to do this
    /// once the windowing system is ready. If this is a headless window, the redraw
    /// will happen immediately.
    fn request_repaint(&self, _: &ServoShellWindow);
    /// Request a new outer size for the window, including external decorations.
    /// This should be the same as `window.outerWidth` and `window.outerHeight``
    fn request_resize(&self, webview: &WebView, outer_size: DeviceIntSize)
    -> Option<DeviceIntSize>;
    fn set_position(&self, _point: DeviceIntPoint) {}
    fn set_fullscreen(&self, _state: bool) {}
    fn set_cursor(&self, _cursor: Cursor) {}
    #[cfg(all(
        feature = "webxr",
        not(any(target_os = "android", target_env = "ohos"))
    ))]
    fn new_glwindow(
        &self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Rc<dyn servo::webxr::GlWindow>;
    /// This returns [`RenderingContext`] matching the viewport.
    fn rendering_context(&self) -> Rc<dyn RenderingContext>;
    fn theme(&self) -> servo::Theme {
        servo::Theme::Light
    }
    fn window_rect(&self) -> DeviceIndependentIntRect;
    fn maximize(&self, _: &WebView) {}
    fn focus(&self) {}
    fn has_platform_focus(&self) -> bool {
        true
    }

    fn show_embedder_control(&self, _: WebViewId, _: EmbedderControl) {}
    fn hide_embedder_control(&self, _: WebViewId, _: EmbedderControlId) {}
    fn dismiss_embedder_controls_for_webview(&self, _: WebViewId) {}
    fn show_bluetooth_device_dialog(
        &self,
        _: WebViewId,
        _request: BluetoothDeviceSelectionRequest,
    ) {
    }
    fn show_permission_dialog(&self, _: WebViewId, _: PermissionRequest) {}
    fn show_http_authentication_dialog(&self, _: WebViewId, _: AuthenticationRequest) {}

    fn notify_input_event_handled(
        &self,
        _webview: &WebView,
        _id: InputEventId,
        _result: InputEventResult,
    ) {
    }

    fn notify_media_session_event(&self, _: MediaSessionEvent) {}
    fn notify_crashed(&self, _: WebView, _reason: String, _backtrace: Option<String>) {}
    fn show_console_message(&self, _level: ConsoleLogLevel, _message: &str) {}

    #[cfg(not(any(target_os = "android", target_env = "ohos")))]
    /// If this window is a headed window, access the concrete type.
    fn as_headed_window(&self) -> Option<&crate::desktop::headed_window::HeadedWindow> {
        None
    }

    #[cfg(any(target_os = "android", target_env = "ohos"))]
    /// If this window is a headed window, access the concrete type.
    fn as_headed_window(&self) -> Option<&crate::egl::app::EmbeddedPlatformWindow> {
        None
    }

    fn notify_accessibility_tree_update(&self, _: WebView, _: accesskit::TreeUpdate) {}
}

#[cfg(test)]
mod tests {
    use super::{ReusableInternalPage, reusable_internal_page};
    use url::Url;

    #[test]
    fn reusable_internal_page_recognizes_main_internal_pages() {
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://home").unwrap()),
            Some(ReusableInternalPage::Home)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:home").unwrap()),
            Some(ReusableInternalPage::Home)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://web").unwrap()),
            Some(ReusableInternalPage::Web)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:web").unwrap()),
            Some(ReusableInternalPage::Web)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://downloads").unwrap()),
            Some(ReusableInternalPage::Downloads)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:downloads").unwrap()),
            Some(ReusableInternalPage::Downloads)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://calendar").unwrap()),
            Some(ReusableInternalPage::Calendar)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:calendar").unwrap()),
            Some(ReusableInternalPage::Calendar)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://chat").unwrap()),
            Some(ReusableInternalPage::Chat)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:chat").unwrap()),
            Some(ReusableInternalPage::Chat)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://messages").unwrap()),
            Some(ReusableInternalPage::Chat)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:messages").unwrap()),
            Some(ReusableInternalPage::Chat)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://settings?chrome_zoom=0.82").unwrap()),
            Some(ReusableInternalPage::Settings)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate:settings").unwrap()),
            Some(ReusableInternalPage::Settings)
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://home/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://web/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://blank").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://downloads/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://calendar/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://chat/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://messages/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("slate://settings/state").unwrap()),
            None
        );
        assert_eq!(
            reusable_internal_page(&Url::parse("https://example.com").unwrap()),
            None
        );
    }

    #[test]
    fn reusable_internal_page_deduplicates_app_pages() {
        assert!(ReusableInternalPage::Home.closes_duplicates());
        assert!(ReusableInternalPage::Web.closes_duplicates());
        assert!(ReusableInternalPage::Downloads.closes_duplicates());
        assert!(ReusableInternalPage::Calendar.closes_duplicates());
        assert!(ReusableInternalPage::Chat.closes_duplicates());
        assert!(ReusableInternalPage::Settings.closes_duplicates());
    }

    #[test]
    fn reusable_internal_page_focuses_newest_existing_app_page() {
        assert_eq!(ReusableInternalPage::Home.existing_target_index(0), None);
        assert_eq!(ReusableInternalPage::Home.existing_target_index(3), Some(2));
        assert_eq!(ReusableInternalPage::Web.existing_target_index(3), Some(2));
        assert_eq!(
            ReusableInternalPage::Downloads.existing_target_index(3),
            Some(2)
        );
        assert_eq!(
            ReusableInternalPage::Calendar.existing_target_index(3),
            Some(2)
        );
        assert_eq!(ReusableInternalPage::Chat.existing_target_index(3), Some(2));
        assert_eq!(
            ReusableInternalPage::Settings.existing_target_index(3),
            Some(2)
        );
    }
}
