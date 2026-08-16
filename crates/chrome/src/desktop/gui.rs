/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::fs;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use dpi::PhysicalSize;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use egui::{FontData, FontFamily};
use egui::{
    FontDefinitions, Id, Key, LayerId, Modifiers, Order, PaintCallback, Panel, Vec2, WidgetInfo,
    WidgetType,
};
use egui_glow::{CallbackFn, EguiGlow};
use egui_winit::EventResponse;
use euclid::{Length, Point2D, Rect, Scale, Size2D};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use log::info;
use log::warn;
use servo::{
    DeviceIndependentPixel, DevicePixel, Image, LoadStatus, OffscreenRenderingContext, PixelFormat,
    RenderingContext, WebView, WebViewId,
};
use slate_broadwebd::{
    BroadwebDaemon, BroadwebStatusKind, BroadwebStatusSnapshot, FetchDisposition, HttpFetchRequest,
    default_session_status_snapshot,
};
use slate_storage::{
    BookmarkRecord, BookmarkUpdate, DEFAULT_HOME_BOOKMARKS, DEFAULT_PROFILE_ID, HistoryVisitRecord,
    SlateProfileDatabase, StorageError,
};
use url::Url;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use crate::desktop::event_loop::AppEvent;
use crate::desktop::headed_window;
use crate::desktop::protocols::slate::{
    self, current_chrome_element_zoom_setting, is_slate_blank_url, is_slate_downloads_url,
    is_slate_home_url, is_slate_settings_url, is_slate_web_url,
    set_current_chrome_element_zoom_setting,
};
use crate::desktop::slate_theme::{self, SlateIcon, SlateIconCache, SlateRaster};
use crate::running_app_state::{RunningAppState, UserInterfaceCommand};
use crate::window::ServoShellWindow;

pub(crate) mod headless_snapshot;

const TAB_STRIP_HEIGHT: f32 = 78.0;
const TAB_STRIP_CONTENT_ALIGN: egui::Align = egui::Align::Max;
const TAB_CONTENT_ALIGN: egui::Align = egui::Align::Center;
const ACTIVE_TAB_BOTTOM_JOIN_HEIGHT: f32 = 4.0;
const ACTIVE_TAB_BOTTOM_JOIN_INSET_X: f32 = 0.0;
const ACTIVE_TAB_FILE_CORNER_STEPS: usize = 5;
const CHROME_ELEMENT_ZOOM: f32 = slate::CHROME_ELEMENT_ZOOM_SETTING_DEFAULT;
const CHROME_ELEMENT_ZOOM_MIN: f32 = slate::CHROME_ELEMENT_ZOOM_SETTING_MIN;
const CHROME_ELEMENT_ZOOM_MAX: f32 = slate::CHROME_ELEMENT_ZOOM_SETTING_MAX;
const TOOLBAR_HEIGHT: f32 = 84.0 * CHROME_ELEMENT_ZOOM;
const APP_RAIL_WIDTH: f32 = 104.0 * CHROME_ELEMENT_ZOOM;
const FOOTER_HEIGHT: f32 = 44.0;
const FOOTER_PANEL_MARGIN_X: i8 = 0;
const FOOTER_PANEL_MARGIN_TOP: i8 = 4;
const FOOTER_PANEL_MARGIN_BOTTOM: i8 = 4;
const FOOTER_LEFT_PADDING: f32 = 16.0;
const FOOTER_RIGHT_PADDING: f32 = 12.0;
const FOOTER_TEXT_SIZE: f32 = 13.0;
const FOOTER_LOAD_STATUS_DOT_SIZE: f32 = 10.0;
const FOOTER_LOAD_STATUS_DOT_LABEL_GAP: f32 = 8.0;
const FOOTER_LOAD_STATUS_HEIGHT: f32 = 28.0;
const APP_TITLE_WIDTH: f32 = 160.0;
const APP_TITLE_HEIGHT: f32 = TAB_STRIP_HEIGHT;
const APP_TITLE_LEFT_PADDING: f32 = 31.0;
const APP_TITLE_TEXT_SIZE: f32 = 28.0;
const TAB_WIDTH: f32 = 308.0;
const TAB_MIN_WIDTH: f32 = 196.0;
const TAB_OPENING_PREFERRED_WIDTH: f32 = 244.0;
const TAB_OPENING_WINDOW_WIDTH: f32 = 1024.0;
const TAB_CONCEPT_WINDOW_WIDTH: f32 = 1672.0;
const TAB_HEIGHT: f32 = 60.0;
const TAB_CORNER_RADIUS: u8 = 8;
const TAB_INNER_MARGIN_X: i8 = 16;
const TAB_INNER_MARGIN_Y: i8 = 8;
const TAB_CONTENT_HEIGHT: f32 = TAB_HEIGHT - (TAB_INNER_MARGIN_Y as f32 * 2.0);
const TAB_TITLE_MIN_WIDTH: f32 = 80.0;
const TAB_TITLE_TEXT_SIZE: f32 = 20.0;
const TAB_ICON_TITLE_GAP: f32 = 12.0;
const TAB_TITLE_CLOSE_GAP: f32 = 8.0;
const TAB_CLOSE_BUTTON_SIZE: f32 = 24.0;
const TAB_CLOSE_BUTTON_RADIUS: u8 = 6;
const TAB_CLOSE_ICON_SIZE: f32 = 12.0;
const NEW_TAB_LEFT_GAP: f32 = 9.0;
const NEW_TAB_SLOT_HEIGHT: f32 = TAB_HEIGHT;
const NEW_TAB_BUTTON_SIZE: f32 = 44.0;
const NEW_TAB_BUTTON_RADIUS: u8 = 8;
const NEW_TAB_ICON_SIZE: f32 = 17.0;
const NEW_TAB_ICON_STROKE: f32 = 2.0;
const TOOLBAR_PANEL_MARGIN_X: i8 = 18;
const TOOLBAR_PANEL_MARGIN_Y: i8 = 10;
const TOOLBAR_ITEM_SPACING: f32 = 20.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_BUTTON_SIZE: f32 = 40.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_BUTTON_RADIUS: u8 = 8;
const TOOLBAR_ICON_SIZE: f32 = 24.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_NAV_ICON_SIZE: f32 = 28.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_NAV_BACK_ICON_OFFSET_X: f32 = 8.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_NAV_FORWARD_ICON_OFFSET_X: f32 = 7.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_NAV_REFRESH_ICON_OFFSET_X: f32 = 6.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_PRIVACY_ICON_SIZE: f32 = 40.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_MENU_ICON_WIDTH: f32 = 20.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_MENU_ICON_OFFSET_X: f32 = -3.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_MENU_ICON_GAP: f32 = 8.5 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_MENU_ICON_STROKE: f32 = 2.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_SEPARATOR_HEIGHT: f32 = 28.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_SEPARATOR_LEADING_GAP: f32 = 18.0 * CHROME_ELEMENT_ZOOM;
const TOOLBAR_SEPARATOR_TRAILING_GAP: f32 = 22.0 * CHROME_ELEMENT_ZOOM;
const RAIL_ICON_SIZE: f32 = 40.0 * CHROME_ELEMENT_ZOOM;
const RAIL_LABEL_TEXT_SIZE: f32 = 12.0 * CHROME_ELEMENT_ZOOM;
const RAIL_ICON_LABEL_GAP: f32 = 6.0 * CHROME_ELEMENT_ZOOM;
const RAIL_BUTTON_SIZE: f32 = 80.0 * CHROME_ELEMENT_ZOOM;
const RAIL_BUTTON_RADIUS: u8 = 8;
const RAIL_PANEL_MARGIN_X: i8 = 12;
const RAIL_PANEL_MARGIN_Y: i8 = 0;
const RAIL_TOP_SPACE: f32 = 22.0 * CHROME_ELEMENT_ZOOM;
const RAIL_ITEM_GAP: f32 = 12.0 * CHROME_ELEMENT_ZOOM;
const TAB_ICON_SIZE: f32 = 32.0;
const ADDRESS_LEADING_GAP: f32 = 20.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_MIN_WIDTH: f32 = 260.0;
const ADDRESS_HEIGHT: f32 = 52.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_TEXT_HEIGHT: f32 = 34.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_INPUT_TEXT_SIZE: f32 = 20.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_CORNER_RADIUS: u8 = 8;
const ADDRESS_INNER_MARGIN_X: i8 = 18;
const ADDRESS_SHADOW_OFFSET: [i8; 2] = [0, 1];
const ADDRESS_SHADOW_BLUR: u8 = 6;
const ADDRESS_SHADOW_SPREAD: u8 = 0;
const ADDRESS_SHADOW_ALPHA: u8 = 6;
const ADDRESS_SECURITY_ICON_SIZE: f32 = 24.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_SLATE_SECURITY_ICON_SIZE: f32 = 34.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_SLATE_SECURITY_ICON_OFFSET_X: f32 = -2.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_ICON_GAP: f32 = 14.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_BOOKMARK_ICON_SIZE: f32 = 22.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_BOOKMARK_BUTTON_SIZE: f32 = 28.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_BOOKMARK_BUTTON_RADIUS: u8 = 6;
const ADDRESS_BOOKMARK_RESERVED_WIDTH: f32 = 28.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_TRAILING_CONTROLS_WIDTH: f32 = 188.0 * CHROME_ELEMENT_ZOOM;
const ADDRESS_TRAILING_GAP: f32 = 6.0 * CHROME_ELEMENT_ZOOM;
const HOME_SEARCH_MIN_WIDTH: f32 = 280.0;
const HOME_SEARCH_MAX_WIDTH: f32 = 880.0;
const HOME_SEARCH_WIDTH_FACTOR: f32 = 0.56;
const HOME_SEARCH_HORIZONTAL_PADDING: f32 = 32.0;
const HOME_SEARCH_HEIGHT: f32 = 72.0;
const HOME_SEARCH_FRAME_EXTRA_HEIGHT: f32 = 8.0;
const HOME_SEARCH_TEXT_HEIGHT: f32 = 34.0;
const HOME_SEARCH_INPUT_TEXT_SIZE: f32 = 20.0;
const HOME_SEARCH_INNER_MARGIN_X: i8 = 28;
const HOME_SEARCH_ICON_SIZE: f32 = 40.0;
const HOME_SEARCH_ICON_OFFSET_Y: f32 = -3.0;
const HOME_SEARCH_ICON_GAP: f32 = 24.0;
const HOME_SEARCH_CORNER_RADIUS: u8 = 8;
const HOME_TOP_SPACE_FACTOR: f32 = 0.18;
const HOME_TOP_SPACE_MIN: f32 = 48.0;
const HOME_TOP_SPACE_MAX: f32 = 132.0;
const HOME_BOTTOM_MIN_GAP: f32 = 16.0;
const HOME_HERO_SIZE: f32 = 78.0;
const HOME_MOTTO_WIDTH: f32 = 280.0;
const HOME_MOTTO_HEIGHT: f32 = 28.0;
const HOME_MOTTO_TEXT_SIZE: f32 = 20.0;
const HOME_HERO_MOTTO_GAP: f32 = 14.0;
const HOME_HERO_TO_SEARCH_GAP: f32 = 41.0;
const HOME_SEARCH_TO_METRICS_GAP: f32 = 57.0;
const HOME_PANEL_SHADOW_OFFSET: [i8; 2] = [0, 2];
const HOME_PANEL_SHADOW_BLUR: u8 = 12;
const HOME_PANEL_SHADOW_SPREAD: u8 = 0;
const HOME_PANEL_SHADOW_ALPHA: u8 = 6;
const HOME_METRIC_CARD_HEIGHT: f32 = 172.0;
const HOME_METRIC_GRID_EXTRA_HEIGHT: f32 = 25.0;
const HOME_METRIC_CARD_MIN_WIDTH: f32 = 156.0;
const HOME_METRIC_CARD_MAX_WIDTH: f32 = 194.0;
const HOME_METRIC_CARD_GAP: f32 = 33.0;
const HOME_METRIC_CARD_INNER_MARGIN_X: i8 = 16;
const HOME_METRIC_CARD_INNER_MARGIN_Y: i8 = 36;
const HOME_METRIC_ICON_SIZE: f32 = 52.0;
const HOME_METRIC_ICON_LABEL_GAP: f32 = 16.0;
const HOME_METRIC_LABEL_TEXT_SIZE: f32 = 16.0;
const HOME_METRIC_DETAIL_TEXT_SIZE: f32 = 13.0;
const HOME_METRIC_DETAIL_GAP: f32 = 4.0;
const HOME_METRIC_BADGE_TEXT_SIZE: f32 = 13.0;
const HOME_METRIC_BADGE_PRIMARY_DIGIT_FACTOR: f32 = 0.58;
const HOME_METRIC_BADGE_EXTRA_DIGIT_FACTOR: f32 = 0.31;
const HOME_METRIC_BADGE_LABEL_GAP: f32 = 8.0;
const HOME_METRIC_BADGE_MARGIN_X: i8 = 8;
const HOME_METRIC_BADGE_MARGIN_Y: i8 = 3;
const HOME_METRIC_BADGE_CORNER_RADIUS: u8 = 10;
const HOME_CONTENT_OPTICAL_OFFSET_X: f32 = -13.0;
const HOME_HERO_OPTICAL_OFFSET_X: f32 = -29.0;
const HOME_BOOKMARK_SLOT_COUNT: usize = 2;
const HOME_BOOKMARK_CARD_COUNT: usize = 4;
const HOME_FAVICON_MAX_BYTES: usize = 256 * 1024;
const HOME_FAVICON_MAX_SIDE: u32 = 64;
const HOME_FAVICON_FETCH_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
const STATUS_BUBBLE_MARGIN_X: f32 = 14.0;
const STATUS_BUBBLE_MARGIN_Y: f32 = 12.0;
const STATUS_BUBBLE_HEIGHT: f32 = 32.0;
const STATUS_BUBBLE_MAX_WIDTH: f32 = 560.0;
const STATUS_BUBBLE_HORIZONTAL_PADDING: f32 = 12.0;
const STATUS_BUBBLE_CORNER_RADIUS: u8 = 8;
const STATUS_BUBBLE_SHADOW_ALPHA: u8 = 8;
const STATUS_TEXT_SIZE: f32 = 13.0;
const SLATE_MOTTO: &str = "Protected. Private. Yours.";

/// The user interface of a headed servoshell. Currently this is implemented via
/// egui.
pub struct Gui {
    rendering_context: Rc<OffscreenRenderingContext>,
    context: EguiGlow,
    toolbar_height: Length<f32, DeviceIndependentPixel>,
    webview_origin: Point2D<f32, DeviceIndependentPixel>,
    webview_size: Size2D<f32, DeviceIndependentPixel>,
    webview_contains_native_chrome: bool,

    location: String,
    home_search: String,
    home_bookmarks: Vec<HomeBookmarkCard>,
    home_bookmarks_loaded: bool,
    web_history_cards: Vec<HomeBookmarkCard>,
    home_favicon_textures: HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
    home_favicon_fetches: HashSet<String>,
    home_favicon_failures: HashSet<String>,
    home_favicon_tx: Sender<HomeFaviconFetchResult>,
    home_favicon_rx: Receiver<HomeFaviconFetchResult>,
    toolbar_menu_popup_id: Option<egui::Id>,

    /// Whether the location has been edited by the user without clicking Go.
    location_dirty: bool,

    /// The [`LoadStatus`] of the active `WebView`.
    load_status: LoadStatus,

    /// The text to display in the status bar on the bottom of the window.
    status_text: Option<String>,

    /// Latest broadwebd status snapshot for protocol-backed page loads.
    broadweb_status: BroadwebStatusSnapshot,

    /// User-adjustable zoom for Slate-owned chrome elements.
    chrome_element_zoom: f32,

    /// Last settings page URL whose `chrome_zoom` query was applied to the shared setting.
    last_chrome_element_zoom_url: Option<String>,

    /// Platform-provided egui zoom compensation for DPI handling.
    platform_zoom_factor: Cell<f32>,

    /// Whether or not the current `WebView` can navigate backward.
    can_go_back: bool,

    /// Whether or not the current `WebView` can navigate forward.
    can_go_forward: bool,

    /// Handle to the GPU texture of the favicon.
    ///
    /// These need to be cached across egui draw calls.
    favicon_textures: HashMap<WebViewId, (egui::TextureHandle, egui::load::SizedTexture)>,

    /// Cached GPU textures for Slate's extracted raster icon masks.
    slate_icons: SlateIconCache,

    /// AccessKit tree updates pending the next egui tick.
    /// This allows us to ensure that graft nodes are sent before the subtrees they graft.
    pending_accesskit_updates: Vec<accesskit::TreeUpdate>,
}

fn truncate_with_ellipsis(input: &str, max_length: usize) -> String {
    if input.chars().count() > max_length {
        let truncated: String = input.chars().take(max_length.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        input.to_string()
    }
}

fn egui_chrome_owns_position(
    webview_origin: Point2D<f32, DeviceIndependentPixel>,
    webview_size: Size2D<f32, DeviceIndependentPixel>,
    webview_contains_native_chrome: bool,
    position: Point2D<f32, DeviceIndependentPixel>,
) -> bool {
    !Rect::new(webview_origin, webview_size).contains(position) || webview_contains_native_chrome
}

fn egui_chrome_captures_mouse_position(
    webview_origin: Point2D<f32, DeviceIndependentPixel>,
    webview_size: Size2D<f32, DeviceIndependentPixel>,
    webview_contains_native_chrome: bool,
    chrome_popup_open: bool,
    position: Point2D<f32, DeviceIndependentPixel>,
) -> bool {
    chrome_popup_open
        || egui_chrome_owns_position(
            webview_origin,
            webview_size,
            webview_contains_native_chrome,
            position,
        )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HomeMetricsLayout {
    columns: usize,
    card_width: f32,
    spacing: f32,
}

#[derive(Clone, Copy, Debug)]
struct HomeContentLayout {
    hero_rect: egui::Rect,
    motto_rect: egui::Rect,
    search_rect: egui::Rect,
    search_icon_rect: egui::Rect,
    metrics_rect: egui::Rect,
}

impl Default for HomeContentLayout {
    fn default() -> Self {
        Self {
            hero_rect: egui::Rect::NOTHING,
            motto_rect: egui::Rect::NOTHING,
            search_rect: egui::Rect::NOTHING,
            search_icon_rect: egui::Rect::NOTHING,
            metrics_rect: egui::Rect::NOTHING,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct HomeBookmarkCard {
    label: String,
    detail: String,
    url: Option<String>,
    favicon_key: Option<String>,
    favicon_url: Option<String>,
}

#[derive(Debug)]
struct HomeContentResponse {
    navigation_request: Option<String>,
    layout: HomeContentLayout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HomeFaviconBytes {
    media_type: Option<String>,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HomeFaviconFetchResult {
    key: String,
    result: Result<HomeFaviconBytes, String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AddressSecurityIcon {
    Slate {
        icon: SlateIcon,
        color: egui::Color32,
    },
    Raster(SlateRaster),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeChromePage {
    Home,
    Web,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RailPage {
    Home,
    Web,
    Downloads,
}

fn home_metrics_layout(available_width: f32) -> HomeMetricsLayout {
    let available_width = available_width.max(0.0);
    let columns: usize = if available_width < 360.0 {
        1
    } else if available_width < 780.0 {
        2
    } else {
        4
    };
    let spacing = HOME_METRIC_CARD_GAP;
    let total_spacing = spacing * (columns.saturating_sub(1) as f32);
    let raw_card_width = (available_width - total_spacing).max(0.0) / columns as f32;
    let max_card_width = HOME_METRIC_CARD_MAX_WIDTH.min(available_width);
    let min_card_width = HOME_METRIC_CARD_MIN_WIDTH.min(max_card_width);
    let card_width = raw_card_width.clamp(min_card_width, max_card_width);

    HomeMetricsLayout {
        columns,
        card_width,
        spacing,
    }
}

fn toolbar_address_width(available_width: f32) -> f32 {
    let available_width = available_width.max(0.0);
    let min_width = ADDRESS_MIN_WIDTH.min(available_width);
    (available_width - ADDRESS_TRAILING_CONTROLS_WIDTH)
        .max(min_width)
        .min(available_width)
}

#[cfg(test)]
fn address_outer_width(content_width: f32) -> f32 {
    content_width + f32::from(ADDRESS_INNER_MARGIN_X) * 2.0
}

fn address_security_icon_for_location(location: &str) -> AddressSecurityIcon {
    if location.trim().is_empty() {
        return AddressSecurityIcon::Raster(SlateRaster::Search);
    }

    match Url::parse(location) {
        Ok(url)
            if is_slate_home_url(&url) || is_slate_web_url(&url) || is_slate_settings_url(&url) =>
        {
            AddressSecurityIcon::Slate {
                icon: SlateIcon::TopShield,
                color: address_passive_icon_color(),
            }
        }
        Ok(url) => match url.scheme() {
            "https" => AddressSecurityIcon::Raster(SlateRaster::PageInfoSecure),
            "http" => AddressSecurityIcon::Raster(SlateRaster::PageInfoInsecure),
            "file" => AddressSecurityIcon::Raster(SlateRaster::PageInfoLocal),
            "about" | "resource" | "servo" | "slate" => {
                AddressSecurityIcon::Raster(SlateRaster::PageInfoInternal)
            }
            _ => AddressSecurityIcon::Raster(SlateRaster::PageInfoWarning),
        },
        Err(_) => AddressSecurityIcon::Raster(SlateRaster::PageInfoWarning),
    }
}

fn address_security_raster_color(raster: SlateRaster) -> egui::Color32 {
    match raster {
        SlateRaster::PageInfoInsecure | SlateRaster::PageInfoWarning => slate_theme::AMBER,
        _ => address_passive_icon_color(),
    }
}

fn address_passive_icon_color() -> egui::Color32 {
    egui::Color32::from_rgb(84, 84, 84)
}

fn address_bookmark_icon_color() -> egui::Color32 {
    address_passive_icon_color()
}

fn address_background_color() -> egui::Color32 {
    slate_theme::FIELD_SURFACE
}

fn address_border_color() -> egui::Color32 {
    slate_theme::FIELD_BORDER
}

fn rail_icon_color(selected: bool) -> egui::Color32 {
    if selected {
        slate_theme::TEAL
    } else {
        slate_theme::TEXT
    }
}

fn rail_selected_button_fill() -> egui::Color32 {
    egui::Color32::from_rgb(236, 240, 239)
}

fn rail_button_fill(selected: bool, hovered: bool) -> egui::Color32 {
    if selected {
        rail_selected_button_fill()
    } else if hovered {
        slate_theme::PANEL_HOVER
    } else {
        egui::Color32::TRANSPARENT
    }
}

fn footer_status_text_color() -> egui::Color32 {
    egui::Color32::from_rgb(57, 58, 55)
}

fn footer_load_status_indicator_color(load_status: LoadStatus) -> egui::Color32 {
    match load_status {
        LoadStatus::Started => egui::Color32::from_rgb(202, 132, 34),
        LoadStatus::HeadParsed => slate_theme::BLUE,
        LoadStatus::Complete => egui::Color32::from_rgb(11, 126, 121),
    }
}

fn footer_load_status_pulse_target_color() -> egui::Color32 {
    egui::Color32::from_rgb(172, 172, 168)
}

fn footer_load_status_is_in_progress(
    load_status: LoadStatus,
    broadweb_status: &BroadwebStatusSnapshot,
) -> bool {
    matches!(load_status, LoadStatus::Started | LoadStatus::HeadParsed)
        || matches!(
            broadweb_status.kind,
            BroadwebStatusKind::Fetching | BroadwebStatusKind::SwitchingGateway
        )
}

fn footer_load_status_base_indicator_color(
    load_status: LoadStatus,
    broadweb_status: &BroadwebStatusSnapshot,
) -> egui::Color32 {
    if matches!(
        broadweb_status.kind,
        BroadwebStatusKind::Fetching | BroadwebStatusKind::SwitchingGateway
    ) && matches!(load_status, LoadStatus::Complete)
    {
        return footer_load_status_indicator_color(LoadStatus::Started);
    }

    footer_load_status_indicator_color(load_status)
}

fn mix_color(from: egui::Color32, to: egui::Color32, amount: f32) -> egui::Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let [from_r, from_g, from_b, from_a] = from.to_array();
    let [to_r, to_g, to_b, to_a] = to.to_array();
    let mix_channel =
        |start: u8, end: u8| f32::from(start) + (f32::from(end) - f32::from(start)) * amount;

    egui::Color32::from_rgba_unmultiplied(
        mix_channel(from_r, to_r).round() as u8,
        mix_channel(from_g, to_g).round() as u8,
        mix_channel(from_b, to_b).round() as u8,
        mix_channel(from_a, to_a).round() as u8,
    )
}

fn footer_load_status_indicator_color_at(
    load_status: LoadStatus,
    broadweb_status: &BroadwebStatusSnapshot,
    time_seconds: f64,
) -> egui::Color32 {
    let base_color = footer_load_status_base_indicator_color(load_status, broadweb_status);
    if !footer_load_status_is_in_progress(load_status, broadweb_status) {
        return base_color;
    }

    let phase = 1.0 - (time_seconds as f32 * std::f32::consts::TAU * 0.8).cos();
    let fade_to_grey = phase / 2.0;
    mix_color(
        base_color,
        footer_load_status_pulse_target_color(),
        fade_to_grey,
    )
}

fn new_tab_icon_color() -> egui::Color32 {
    slate_theme::TEXT
}

fn toolbar_menu_icon_color(_selected: bool) -> egui::Color32 {
    slate_theme::TEXT
}

fn chrome_vertical_separator_color() -> egui::Color32 {
    egui::Color32::from_rgb(225, 225, 225)
}

fn footer_top_separator_color() -> egui::Color32 {
    egui::Color32::from_rgb(241, 240, 239)
}

fn chrome_panel_background_color() -> egui::Color32 {
    slate_theme::CHROME_BG
}

fn tab_strip_background_color() -> egui::Color32 {
    slate_theme::TITLE_SURFACE
}

fn tab_strip_separator_color() -> egui::Color32 {
    slate_theme::FIELD_BORDER
}

fn toolbar_background_color() -> egui::Color32 {
    slate_theme::FIELD_SURFACE
}

fn app_title_background_color() -> egui::Color32 {
    slate_theme::TITLE_SURFACE
}

fn app_title_text_color() -> egui::Color32 {
    egui::Color32::from_rgb(29, 29, 26)
}

fn address_shadow() -> egui::Shadow {
    egui::Shadow {
        offset: ADDRESS_SHADOW_OFFSET,
        blur: ADDRESS_SHADOW_BLUR,
        spread: ADDRESS_SHADOW_SPREAD,
        color: egui::Color32::from_black_alpha(ADDRESS_SHADOW_ALPHA),
    }
}

fn address_slate_security_icon_rect(slot_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        slot_rect.center() + egui::vec2(ADDRESS_SLATE_SECURITY_ICON_OFFSET_X, 0.0),
        egui::Vec2::splat(ADDRESS_SLATE_SECURITY_ICON_SIZE),
    )
}

#[cfg(test)]
fn address_slate_security_visible_rect(icon_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            icon_rect.left() + 7.0 / 28.0 * icon_rect.width(),
            icon_rect.top() + 5.0 / 28.0 * icon_rect.height(),
        ),
        egui::pos2(
            icon_rect.left() + 22.0 / 28.0 * icon_rect.width(),
            icon_rect.top() + 23.0 / 28.0 * icon_rect.height(),
        ),
    )
}

#[cfg(test)]
fn address_bookmark_icon_rect(button_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        button_rect.center(),
        egui::Vec2::splat(ADDRESS_BOOKMARK_ICON_SIZE),
    )
}

fn home_search_width(available_width: f32) -> f32 {
    let padded_width = (available_width - HOME_SEARCH_HORIZONTAL_PADDING).max(0.0);
    let proportional_width = (available_width.max(0.0) * HOME_SEARCH_WIDTH_FACTOR)
        .min(HOME_SEARCH_MAX_WIDTH)
        .min(padded_width);
    proportional_width.max(HOME_SEARCH_MIN_WIDTH.min(padded_width))
}

fn home_search_content_width(search_width: f32) -> f32 {
    (search_width - f32::from(HOME_SEARCH_INNER_MARGIN_X) * 2.0).max(0.0)
}

fn home_search_icon_rect(slot_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        slot_rect.center() + egui::vec2(0.0, HOME_SEARCH_ICON_OFFSET_Y),
        egui::Vec2::splat(HOME_SEARCH_ICON_SIZE),
    )
}

#[cfg(test)]
fn home_search_icon_visible_rect(icon_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            icon_rect.left() + 7.0 / 32.0 * icon_rect.width(),
            icon_rect.top() + 6.0 / 32.0 * icon_rect.height(),
        ),
        egui::pos2(
            icon_rect.left() + 30.0 / 32.0 * icon_rect.width(),
            icon_rect.top() + 28.0 / 32.0 * icon_rect.height(),
        ),
    )
}

#[cfg(test)]
fn home_hero_icon_visible_rect(icon_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            icon_rect.left() + 8.0 / 64.0 * icon_rect.width(),
            icon_rect.top() + 4.0 / 64.0 * icon_rect.height(),
        ),
        egui::pos2(
            icon_rect.left() + 56.0 / 64.0 * icon_rect.width(),
            icon_rect.top() + 63.0 / 64.0 * icon_rect.height(),
        ),
    )
}

fn home_metric_card_content_width(card_width: f32) -> f32 {
    (card_width - f32::from(HOME_METRIC_CARD_INNER_MARGIN_X) * 2.0).max(0.0)
}

fn home_metric_card_content_height() -> f32 {
    (HOME_METRIC_CARD_HEIGHT - f32::from(HOME_METRIC_CARD_INNER_MARGIN_Y) * 2.0).max(0.0)
}

fn home_metric_badge_width(text: &str) -> f32 {
    let digit_count = text.chars().count();
    let primary_digits = digit_count.min(2) as f32;
    let extra_digits = digit_count.saturating_sub(2) as f32;

    (primary_digits * HOME_METRIC_BADGE_PRIMARY_DIGIT_FACTOR
        + extra_digits * HOME_METRIC_BADGE_EXTRA_DIGIT_FACTOR)
        * HOME_METRIC_BADGE_TEXT_SIZE
        + f32::from(HOME_METRIC_BADGE_MARGIN_X) * 2.0
}

fn home_search_rendered_height() -> f32 {
    HOME_SEARCH_HEIGHT + HOME_SEARCH_FRAME_EXTRA_HEIGHT
}

fn home_metrics_rendered_height() -> f32 {
    HOME_METRIC_CARD_HEIGHT + HOME_METRIC_GRID_EXTRA_HEIGHT
}

fn home_panel_shadow() -> egui::Shadow {
    egui::Shadow {
        offset: HOME_PANEL_SHADOW_OFFSET,
        blur: HOME_PANEL_SHADOW_BLUR,
        spread: HOME_PANEL_SHADOW_SPREAD,
        color: egui::Color32::from_black_alpha(HOME_PANEL_SHADOW_ALPHA),
    }
}

fn home_view_background_color() -> egui::Color32 {
    slate_theme::HOME_BG
}

fn home_search_background_color() -> egui::Color32 {
    slate_theme::FIELD_SURFACE
}

fn home_search_border_color() -> egui::Color32 {
    slate_theme::BORDER
}

fn home_search_icon_color() -> egui::Color32 {
    egui::Color32::from_rgb(88, 87, 89)
}

fn home_metric_card_background_color() -> egui::Color32 {
    slate_theme::HOME_BG
}

fn home_metric_detail_color() -> egui::Color32 {
    egui::Color32::from_rgb(145, 144, 144)
}

fn home_bookmark_placeholder_cards() -> Vec<HomeBookmarkCard> {
    vec![
        HomeBookmarkCard {
            label: "Add bookmark".to_string(),
            detail: "Save a favorite site".to_string(),
            url: None,
            favicon_key: None,
            favicon_url: None,
        },
        HomeBookmarkCard {
            label: "Add another".to_string(),
            detail: "Pin your broadweb".to_string(),
            url: None,
            favicon_key: None,
            favicon_url: None,
        },
    ]
}

fn web_history_placeholder_cards() -> Vec<HomeBookmarkCard> {
    vec![
        HomeBookmarkCard {
            label: "No history yet".to_string(),
            detail: "Visit a website".to_string(),
            url: None,
            favicon_key: None,
            favicon_url: None,
        },
        HomeBookmarkCard {
            label: "Recent sites".to_string(),
            detail: "Stored locally".to_string(),
            url: None,
            favicon_key: None,
            favicon_url: None,
        },
        HomeBookmarkCard {
            label: "Broadweb".to_string(),
            detail: "HTTP, IPFS, IPNS".to_string(),
            url: None,
            favicon_key: None,
            favicon_url: None,
        },
        HomeBookmarkCard {
            label: "Refresh later".to_string(),
            detail: "Manual discovery".to_string(),
            url: None,
            favicon_key: None,
            favicon_url: None,
        },
    ]
}

fn default_home_bookmark_cards() -> Vec<HomeBookmarkCard> {
    let mut bookmarks: Vec<_> = DEFAULT_HOME_BOOKMARKS
        .iter()
        .map(|bookmark| home_bookmark_card(bookmark.title.to_string(), bookmark.url.to_string()))
        .collect();
    fill_home_bookmark_placeholders(&mut bookmarks);
    bookmarks
}

fn default_web_history_cards() -> Vec<HomeBookmarkCard> {
    let mut history = Vec::new();
    fill_web_history_placeholders(&mut history);
    history
}

fn home_bookmark_cards_from_database(
    database: &SlateProfileDatabase,
) -> Result<Vec<HomeBookmarkCard>, slate_storage::StorageError> {
    let mut bookmarks: Vec<_> = home_bookmark_records_from_database(database)?
        .into_iter()
        .map(home_bookmark_card_from_record)
        .collect();
    fill_home_bookmark_placeholders(&mut bookmarks);
    Ok(bookmarks)
}

fn web_history_cards_from_database(
    database: &SlateProfileDatabase,
) -> Result<Vec<HomeBookmarkCard>, slate_storage::StorageError> {
    Ok(web_history_cards_from_records(
        database.recent_history(DEFAULT_PROFILE_ID, 16)?,
    ))
}

fn web_history_cards_from_records(records: Vec<HistoryVisitRecord>) -> Vec<HomeBookmarkCard> {
    let mut history: Vec<_> = records
        .into_iter()
        .filter_map(web_history_card_from_record)
        .take(HOME_BOOKMARK_CARD_COUNT)
        .collect();
    fill_web_history_placeholders(&mut history);
    history
}

fn home_bookmark_records_from_database(
    database: &SlateProfileDatabase,
) -> Result<Vec<BookmarkRecord>, StorageError> {
    Ok(database
        .bookmarks(DEFAULT_PROFILE_ID)?
        .into_iter()
        .take(HOME_BOOKMARK_SLOT_COUNT)
        .collect())
}

fn fill_home_bookmark_placeholders(bookmarks: &mut Vec<HomeBookmarkCard>) {
    for placeholder in home_bookmark_placeholder_cards() {
        if bookmarks.len() >= HOME_BOOKMARK_CARD_COUNT {
            break;
        }
        bookmarks.push(placeholder);
    }
}

fn fill_web_history_placeholders(history: &mut Vec<HomeBookmarkCard>) {
    for placeholder in web_history_placeholder_cards() {
        if history.len() >= HOME_BOOKMARK_CARD_COUNT {
            break;
        }
        history.push(placeholder);
    }
}

fn home_bookmark_card_from_record(record: BookmarkRecord) -> HomeBookmarkCard {
    let label = record
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| home_bookmark_detail(&record.url));
    let mut card = home_bookmark_card(label, record.url);
    card.favicon_key = card.url.as_deref().and_then(|url| {
        record
            .favicon_key
            .filter(|key| !key.trim().is_empty())
            .or_else(|| Some(home_bookmark_favicon_key(url)))
    });
    card
}

fn web_history_card_from_record(record: HistoryVisitRecord) -> Option<HomeBookmarkCard> {
    if !is_home_bookmarkable_url(&record.url) {
        return None;
    }

    Some(home_bookmark_card(
        record
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| home_bookmark_detail(&record.url)),
        record.url,
    ))
}

fn home_bookmark_card(label: String, url: String) -> HomeBookmarkCard {
    HomeBookmarkCard {
        label,
        detail: home_bookmark_detail(&url),
        favicon_key: Some(home_bookmark_favicon_key(&url)),
        favicon_url: home_bookmark_favicon_url(&url),
        url: Some(url),
    }
}

fn home_bookmark_detail(url: &str) -> String {
    Url::parse(url).ok().map_or_else(
        || url.to_string(),
        |parsed| match parsed.scheme() {
            "ipfs" | "ipns" => parsed
                .host_str()
                .map(|host| format!("{}://{host}", parsed.scheme()))
                .unwrap_or_else(|| url.to_string()),
            "http" | "https" => parsed
                .host_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| url.to_string()),
            _ => url.to_string(),
        },
    )
}

fn home_bookmark_favicon_key(url: &str) -> String {
    let cache_url = home_bookmark_favicon_url(url).unwrap_or_else(|| url.to_string());
    format!("favicon:{cache_url}")
}

fn home_bookmark_favicon_url(url: &str) -> Option<String> {
    let mut parsed = Url::parse(url).ok()?;
    if !home_bookmark_scheme_can_fetch_favicon(parsed.scheme()) {
        return None;
    }

    parsed.set_path("/favicon.ico");
    parsed.set_query(None);
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

fn is_default_home_bookmark_url(url: &str) -> bool {
    DEFAULT_HOME_BOOKMARKS
        .iter()
        .any(|bookmark| bookmark.url == url)
}

fn home_bookmark_slot_for_url(bookmarks: &[BookmarkRecord], url: &str) -> usize {
    if let Some(index) = bookmarks.iter().position(|bookmark| bookmark.url == url) {
        return index.min(HOME_BOOKMARK_SLOT_COUNT.saturating_sub(1));
    }

    if let Some(index) = bookmarks
        .iter()
        .position(|bookmark| is_default_home_bookmark_url(&bookmark.url))
    {
        return index.min(HOME_BOOKMARK_SLOT_COUNT.saturating_sub(1));
    }

    bookmarks
        .len()
        .min(HOME_BOOKMARK_SLOT_COUNT.saturating_sub(1))
}

fn home_bookmark_title(page_title: Option<String>, url: &str) -> String {
    page_title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| home_bookmark_detail(url))
}

fn is_home_bookmarkable_url(url: &str) -> bool {
    Url::parse(url).ok().is_some_and(|url| {
        !matches!(
            url.scheme(),
            "about" | "data" | "file" | "javascript" | "resource" | "servo" | "slate"
        )
    })
}

fn home_bookmark_scheme_can_fetch_favicon(scheme: &str) -> bool {
    matches!(scheme, "http" | "https" | "ipfs" | "ipns")
}

fn status_bubble_width(text: &str, available_width: f32) -> f32 {
    let max_width = STATUS_BUBBLE_MAX_WIDTH.min(available_width.max(0.0));
    let measured_width = text.chars().count() as f32 * STATUS_TEXT_SIZE * 0.62
        + STATUS_BUBBLE_HORIZONTAL_PADDING * 2.0;
    measured_width.min(max_width)
}

fn status_bubble_label(text: &str, bubble_width: f32) -> String {
    let text_width = (bubble_width - STATUS_BUBBLE_HORIZONTAL_PADDING * 2.0).max(0.0);
    let max_chars = (text_width / (STATUS_TEXT_SIZE * 0.62)).floor().max(1.0) as usize;
    truncate_with_ellipsis(text, max_chars)
}

fn home_content_fixed_height() -> f32 {
    HOME_HERO_SIZE
        + HOME_HERO_MOTTO_GAP
        + HOME_MOTTO_HEIGHT
        + HOME_HERO_TO_SEARCH_GAP
        + home_search_rendered_height()
        + HOME_SEARCH_TO_METRICS_GAP
        + home_metrics_rendered_height()
        + HOME_BOTTOM_MIN_GAP
}

fn home_metrics_row_width(layout: HomeMetricsLayout) -> f32 {
    layout.card_width * layout.columns as f32
        + layout.spacing * (layout.columns.saturating_sub(1) as f32)
}

fn home_content_left_space_with_offset(
    available_width: f32,
    content_width: f32,
    offset_x: f32,
) -> f32 {
    let available_width = available_width.max(0.0);
    let content_width = content_width.max(0.0);
    let max_left_space = (available_width - content_width).max(0.0);
    let centered_left_space = max_left_space / 2.0;

    (centered_left_space + offset_x).clamp(0.0, max_left_space)
}

fn home_content_left_space(available_width: f32, content_width: f32) -> f32 {
    home_content_left_space_with_offset(
        available_width,
        content_width,
        HOME_CONTENT_OPTICAL_OFFSET_X,
    )
}

fn home_hero_left_space(available_width: f32, content_width: f32) -> f32 {
    home_content_left_space_with_offset(available_width, content_width, HOME_HERO_OPTICAL_OFFSET_X)
}

fn location_has_broadweb_status(location: &str) -> bool {
    location.starts_with("ipfs://") || location.starts_with("ipns://")
}

fn location_matches_slate_url(location: &str, predicate: fn(&Url) -> bool) -> bool {
    Url::parse(location).ok().is_some_and(|url| predicate(&url))
}

fn location_is_home(location: &str) -> bool {
    location_matches_slate_url(location, is_slate_home_url)
}

fn location_is_web(location: &str) -> bool {
    location_matches_slate_url(location, is_slate_web_url)
}

fn location_is_downloads(location: &str) -> bool {
    location_matches_slate_url(location, is_slate_downloads_url)
}

fn location_for_toolbar(url: &Url) -> String {
    if is_slate_blank_url(url) {
        String::new()
    } else {
        url.to_string()
    }
}

fn clamp_chrome_element_zoom(zoom: f32) -> f32 {
    slate::clamp_chrome_element_zoom_setting(zoom)
}

fn chrome_element_zoom_from_settings_url(url: &Url) -> Option<f32> {
    slate::chrome_element_zoom_setting_from_url(url)
}

fn chrome_element_zoom_factor(chrome_element_zoom: f32) -> f32 {
    clamp_chrome_element_zoom(chrome_element_zoom) / CHROME_ELEMENT_ZOOM
}

#[cfg(test)]
fn home_content_stack_height(available_height: f32) -> f32 {
    home_top_space(available_height)
        + HOME_HERO_SIZE
        + HOME_HERO_MOTTO_GAP
        + HOME_MOTTO_HEIGHT
        + HOME_HERO_TO_SEARCH_GAP
        + home_search_rendered_height()
        + HOME_SEARCH_TO_METRICS_GAP
        + home_metrics_rendered_height()
}

fn home_top_space(available_height: f32) -> f32 {
    let available_height = available_height.max(0.0);
    let preferred =
        (available_height * HOME_TOP_SPACE_FACTOR).clamp(HOME_TOP_SPACE_MIN, HOME_TOP_SPACE_MAX);
    let max_without_clipping = (available_height - home_content_fixed_height()).max(0.0);

    preferred.min(max_without_clipping)
}

#[cfg(test)]
const CONCEPT_SCREENSHOT_WIDTH: f32 = 1672.0;

#[cfg(test)]
const CONCEPT_SCREENSHOT_HEIGHT: f32 = 941.0;

#[cfg(test)]
fn default_opening_home_view_height() -> f32 {
    740.0 - TAB_STRIP_HEIGHT - TOOLBAR_HEIGHT - FOOTER_HEIGHT
}

#[cfg(test)]
fn default_opening_home_view_size() -> egui::Vec2 {
    egui::vec2(1024.0 - APP_RAIL_WIDTH, default_opening_home_view_height())
}

#[cfg(test)]
fn concept_screenshot_home_view_size() -> egui::Vec2 {
    egui::vec2(
        CONCEPT_SCREENSHOT_WIDTH - APP_RAIL_WIDTH,
        CONCEPT_SCREENSHOT_HEIGHT - TAB_STRIP_HEIGHT - TOOLBAR_HEIGHT - FOOTER_HEIGHT,
    )
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct ConceptChromeGeometry {
    tab_strip_rect: egui::Rect,
    app_title_rect: egui::Rect,
    tab_rects: [egui::Rect; 3],
    new_tab_slot_rect: egui::Rect,
    new_tab_button_rect: egui::Rect,
    rail_button_rects: [egui::Rect; 5],
    app_rail_rect: egui::Rect,
    toolbar_rect: egui::Rect,
    toolbar_content_rect: egui::Rect,
    webview_rect: egui::Rect,
    footer_rect: egui::Rect,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct ConceptToolbarControlsGeometry {
    nav_button_rects: [egui::Rect; 3],
    nav_icon_rects: [egui::Rect; 3],
    address_rect: egui::Rect,
    address_security_slot_rect: egui::Rect,
    address_slate_security_icon_rect: egui::Rect,
    address_text_rect: egui::Rect,
    address_bookmark_button_rect: egui::Rect,
    address_bookmark_icon_rect: egui::Rect,
    privacy_button_rect: egui::Rect,
    separator_rect: egui::Rect,
    menu_button_rect: egui::Rect,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct ConceptFooterControlsGeometry {
    load_status_rect: egui::Rect,
    load_status_dot_center: egui::Pos2,
}

#[cfg(test)]
fn concept_chrome_geometry() -> ConceptChromeGeometry {
    let tab_width = tab_width_for_strip(CONCEPT_SCREENSHOT_WIDTH - APP_TITLE_WIDTH, 3);
    let tab_top = TAB_STRIP_HEIGHT - TAB_HEIGHT;
    let first_tab_left = APP_TITLE_WIDTH;
    let tab_rects = [
        egui::Rect::from_min_size(
            egui::pos2(first_tab_left, tab_top),
            egui::vec2(tab_width, TAB_HEIGHT),
        ),
        egui::Rect::from_min_size(
            egui::pos2(first_tab_left + tab_width, tab_top),
            egui::vec2(tab_width, TAB_HEIGHT),
        ),
        egui::Rect::from_min_size(
            egui::pos2(first_tab_left + tab_width * 2.0, tab_top),
            egui::vec2(tab_width, TAB_HEIGHT),
        ),
    ];
    let new_tab_slot_rect = egui::Rect::from_min_size(
        egui::pos2(tab_rects[2].right() + NEW_TAB_LEFT_GAP, tab_top),
        egui::vec2(NEW_TAB_BUTTON_SIZE, NEW_TAB_SLOT_HEIGHT),
    );
    let new_tab_button_rect = egui::Rect::from_center_size(
        new_tab_slot_rect.center(),
        egui::vec2(NEW_TAB_BUTTON_SIZE, NEW_TAB_BUTTON_SIZE),
    );
    let central_width = CONCEPT_SCREENSHOT_WIDTH - APP_RAIL_WIDTH;
    let toolbar_rect = egui::Rect::from_min_size(
        egui::pos2(APP_RAIL_WIDTH, TAB_STRIP_HEIGHT),
        egui::vec2(central_width, TOOLBAR_HEIGHT),
    );
    let rail_button_left = f32::from(RAIL_PANEL_MARGIN_X);
    let first_rail_button_top = TAB_STRIP_HEIGHT + RAIL_TOP_SPACE;
    let rail_step = RAIL_BUTTON_SIZE + RAIL_ITEM_GAP;
    let rail_button_rects = [
        egui::Rect::from_min_size(
            egui::pos2(rail_button_left, first_rail_button_top),
            egui::vec2(RAIL_BUTTON_SIZE, RAIL_BUTTON_SIZE),
        ),
        egui::Rect::from_min_size(
            egui::pos2(rail_button_left, first_rail_button_top + rail_step),
            egui::vec2(RAIL_BUTTON_SIZE, RAIL_BUTTON_SIZE),
        ),
        egui::Rect::from_min_size(
            egui::pos2(rail_button_left, first_rail_button_top + rail_step * 2.0),
            egui::vec2(RAIL_BUTTON_SIZE, RAIL_BUTTON_SIZE),
        ),
        egui::Rect::from_min_size(
            egui::pos2(rail_button_left, first_rail_button_top + rail_step * 3.0),
            egui::vec2(RAIL_BUTTON_SIZE, RAIL_BUTTON_SIZE),
        ),
        egui::Rect::from_min_size(
            egui::pos2(rail_button_left, first_rail_button_top + rail_step * 4.0),
            egui::vec2(RAIL_BUTTON_SIZE, RAIL_BUTTON_SIZE),
        ),
    ];

    ConceptChromeGeometry {
        tab_strip_rect: egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(CONCEPT_SCREENSHOT_WIDTH, TAB_STRIP_HEIGHT),
        ),
        app_title_rect: egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(APP_TITLE_WIDTH, APP_TITLE_HEIGHT),
        ),
        tab_rects,
        new_tab_slot_rect,
        new_tab_button_rect,
        rail_button_rects,
        app_rail_rect: egui::Rect::from_min_size(
            egui::pos2(0.0, TAB_STRIP_HEIGHT),
            egui::vec2(APP_RAIL_WIDTH, CONCEPT_SCREENSHOT_HEIGHT - TAB_STRIP_HEIGHT),
        ),
        toolbar_rect,
        toolbar_content_rect: toolbar_rect.shrink2(egui::vec2(
            f32::from(TOOLBAR_PANEL_MARGIN_X),
            f32::from(TOOLBAR_PANEL_MARGIN_Y),
        )),
        webview_rect: egui::Rect::from_min_size(
            egui::pos2(APP_RAIL_WIDTH, TAB_STRIP_HEIGHT + TOOLBAR_HEIGHT),
            concept_screenshot_home_view_size(),
        ),
        footer_rect: egui::Rect::from_min_size(
            egui::pos2(APP_RAIL_WIDTH, CONCEPT_SCREENSHOT_HEIGHT - FOOTER_HEIGHT),
            egui::vec2(central_width, FOOTER_HEIGHT),
        ),
    }
}

#[cfg(test)]
fn concept_footer_controls_geometry() -> ConceptFooterControlsGeometry {
    let footer_rect = concept_chrome_geometry().footer_rect;
    let margin = footer_panel_margin();
    let footer_content_rect = egui::Rect::from_min_max(
        egui::pos2(
            footer_rect.left() + f32::from(margin.left),
            footer_rect.top() + f32::from(margin.top),
        ),
        egui::pos2(
            footer_rect.right() - f32::from(margin.right),
            footer_rect.bottom() - f32::from(margin.bottom),
        ),
    );
    let center_y = footer_content_rect.center().y;
    let load_status_width =
        footer_content_rect.width() - FOOTER_LEFT_PADDING - FOOTER_RIGHT_PADDING;
    let load_status_rect = egui::Rect::from_min_size(
        egui::pos2(
            footer_content_rect.left() + FOOTER_LEFT_PADDING,
            center_y - FOOTER_LOAD_STATUS_HEIGHT / 2.0,
        ),
        egui::vec2(
            footer_load_status_width(load_status_width),
            FOOTER_LOAD_STATUS_HEIGHT,
        ),
    );
    let load_status_dot_center = egui::pos2(
        load_status_rect.left() + FOOTER_LOAD_STATUS_DOT_SIZE / 2.0,
        load_status_rect.center().y,
    );

    ConceptFooterControlsGeometry {
        load_status_rect,
        load_status_dot_center,
    }
}

#[cfg(test)]
fn concept_toolbar_controls_geometry() -> ConceptToolbarControlsGeometry {
    let toolbar_content_rect = concept_chrome_geometry().toolbar_content_rect;
    let center_y = toolbar_content_rect.center().y;
    let button_top = center_y - TOOLBAR_BUTTON_SIZE / 2.0;
    let nav_button_left = toolbar_content_rect.left();
    let nav_step = TOOLBAR_BUTTON_SIZE + TOOLBAR_ITEM_SPACING;
    let nav_button_rects = [
        egui::Rect::from_min_size(
            egui::pos2(nav_button_left, button_top),
            egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
        ),
        egui::Rect::from_min_size(
            egui::pos2(nav_button_left + nav_step, button_top),
            egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
        ),
        egui::Rect::from_min_size(
            egui::pos2(nav_button_left + nav_step * 2.0, button_top),
            egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
        ),
    ];
    let nav_icon_rects = [
        toolbar_navigation_icon_rect(nav_button_rects[0], SlateIcon::NavBack),
        toolbar_navigation_icon_rect(nav_button_rects[1], SlateIcon::NavForward),
        toolbar_navigation_icon_rect(nav_button_rects[2], SlateIcon::NavRefresh),
    ];
    let address_left = nav_button_rects[2].right() + TOOLBAR_ITEM_SPACING + ADDRESS_LEADING_GAP;
    let address_available_width = (toolbar_content_rect.right() - address_left).max(0.0);
    let address_width = address_outer_width(toolbar_address_width(address_available_width));
    let address_rect = egui::Rect::from_min_size(
        egui::pos2(address_left, center_y - ADDRESS_HEIGHT / 2.0),
        egui::vec2(address_width, ADDRESS_HEIGHT),
    );
    let address_content_left = address_rect.left() + f32::from(ADDRESS_INNER_MARGIN_X);
    let address_content_right = address_rect.right() - f32::from(ADDRESS_INNER_MARGIN_X);
    let address_security_slot_rect = egui::Rect::from_min_size(
        egui::pos2(
            address_content_left,
            center_y - ADDRESS_SECURITY_ICON_SIZE / 2.0,
        ),
        egui::Vec2::splat(ADDRESS_SECURITY_ICON_SIZE),
    );
    let address_slate_security_icon_rect =
        address_slate_security_icon_rect(address_security_slot_rect);
    let address_text_left = address_security_slot_rect.right() + ADDRESS_ICON_GAP;
    let address_text_width =
        (address_content_right - address_text_left - ADDRESS_BOOKMARK_RESERVED_WIDTH).max(80.0);
    let address_text_rect = egui::Rect::from_min_size(
        egui::pos2(address_text_left, center_y - ADDRESS_TEXT_HEIGHT / 2.0),
        egui::vec2(address_text_width, ADDRESS_TEXT_HEIGHT),
    );
    let address_bookmark_button_rect = egui::Rect::from_min_size(
        egui::pos2(
            address_text_rect.right(),
            center_y - ADDRESS_BOOKMARK_BUTTON_SIZE / 2.0,
        ),
        egui::Vec2::splat(ADDRESS_BOOKMARK_BUTTON_SIZE),
    );
    let address_bookmark_icon_rect = address_bookmark_icon_rect(address_bookmark_button_rect);
    let privacy_button_left = address_rect.right() + TOOLBAR_ITEM_SPACING + ADDRESS_TRAILING_GAP;
    let privacy_button_rect = egui::Rect::from_min_size(
        egui::pos2(privacy_button_left, button_top),
        egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
    );
    let separator_left = privacy_button_rect.right() + TOOLBAR_SEPARATOR_LEADING_GAP;
    let separator_rect = egui::Rect::from_min_size(
        egui::pos2(separator_left, center_y - TOOLBAR_SEPARATOR_HEIGHT / 2.0),
        egui::vec2(1.0, TOOLBAR_SEPARATOR_HEIGHT),
    );
    let menu_button_left = separator_rect.right() + TOOLBAR_SEPARATOR_TRAILING_GAP;
    let menu_button_rect = egui::Rect::from_min_size(
        egui::pos2(menu_button_left, button_top),
        egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
    );

    ConceptToolbarControlsGeometry {
        nav_button_rects,
        nav_icon_rects,
        address_rect,
        address_security_slot_rect,
        address_slate_security_icon_rect,
        address_text_rect,
        address_bookmark_button_rect,
        address_bookmark_icon_rect,
        privacy_button_rect,
        separator_rect,
        menu_button_rect,
    }
}

fn tab_preferred_width_for_window(window_width: f32) -> f32 {
    let interpolation = ((window_width - TAB_OPENING_WINDOW_WIDTH)
        / (TAB_CONCEPT_WINDOW_WIDTH - TAB_OPENING_WINDOW_WIDTH))
        .clamp(0.0, 1.0);

    TAB_OPENING_PREFERRED_WIDTH + (TAB_WIDTH - TAB_OPENING_PREFERRED_WIDTH) * interpolation
}

fn tab_width_for_strip(available_width: f32, tab_count: usize) -> f32 {
    let available_width = available_width.max(0.0);
    let preferred = tab_preferred_width_for_window(available_width + APP_TITLE_WIDTH);
    if tab_count == 0 {
        return preferred;
    }

    let available_for_tabs = (available_width - NEW_TAB_LEFT_GAP - NEW_TAB_BUTTON_SIZE).max(0.0);
    let fitting_width = available_for_tabs / tab_count as f32;
    preferred.min(fitting_width.max(TAB_MIN_WIDTH))
}

fn tab_content_width(tab_width: f32) -> f32 {
    (tab_width - f32::from(TAB_INNER_MARGIN_X) * 2.0).max(0.0)
}

#[cfg(test)]
fn tab_icon_slot_rect(tab_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        egui::pos2(
            tab_rect.left() + f32::from(TAB_INNER_MARGIN_X) + TAB_ICON_SIZE / 2.0,
            tab_rect.center().y,
        ),
        egui::Vec2::splat(TAB_ICON_SIZE),
    )
}

#[cfg(test)]
fn tab_title_left(tab_rect: egui::Rect) -> f32 {
    tab_icon_slot_rect(tab_rect).right() + TAB_ICON_TITLE_GAP
}

fn tab_title_width(available_width: f32) -> f32 {
    (available_width
        - TAB_ICON_SIZE
        - TAB_ICON_TITLE_GAP
        - TAB_CLOSE_BUTTON_SIZE
        - TAB_TITLE_CLOSE_GAP)
        .max(TAB_TITLE_MIN_WIDTH)
}

#[cfg(test)]
fn tab_close_button_rect(tab_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        egui::pos2(
            tab_rect.right() - f32::from(TAB_INNER_MARGIN_X) - TAB_CLOSE_BUTTON_SIZE / 2.0,
            tab_rect.center().y,
        ),
        egui::Vec2::splat(TAB_CLOSE_BUTTON_SIZE),
    )
}

fn active_tab_background_color() -> egui::Color32 {
    slate_theme::SURFACE
}

fn inactive_tab_background_color() -> egui::Color32 {
    slate_theme::PANEL
}

fn inactive_tab_hover_background_color() -> egui::Color32 {
    slate_theme::PANEL_HOVER
}

fn inactive_tab_outline_color() -> egui::Color32 {
    slate_theme::BORDER
}

fn active_tab_outline_color() -> egui::Color32 {
    slate_theme::BORDER
}

fn tab_title_color(_active: bool) -> egui::Color32 {
    slate_theme::TEXT
}

fn tab_icon_color(_active: bool) -> egui::Color32 {
    slate_theme::TEXT
}

fn tab_close_icon_color(_active: bool) -> egui::Color32 {
    slate_theme::TEXT
}

fn tab_close_raster(_active: bool) -> SlateRaster {
    SlateRaster::TabClose
}

fn toolbar_navigation_icon_color(_enabled: bool) -> egui::Color32 {
    slate_theme::TEXT
}

fn toolbar_navigation_icon_offset_x(icon: SlateIcon) -> f32 {
    match icon {
        SlateIcon::NavBack => TOOLBAR_NAV_BACK_ICON_OFFSET_X,
        SlateIcon::NavForward => TOOLBAR_NAV_FORWARD_ICON_OFFSET_X,
        SlateIcon::NavRefresh => TOOLBAR_NAV_REFRESH_ICON_OFFSET_X,
        _ => 0.0,
    }
}

fn toolbar_navigation_icon_rect(button_rect: egui::Rect, icon: SlateIcon) -> egui::Rect {
    egui::Rect::from_center_size(
        button_rect.center() + egui::vec2(toolbar_navigation_icon_offset_x(icon), 0.0),
        Vec2::splat(TOOLBAR_NAV_ICON_SIZE),
    )
}

fn toolbar_navigation_raster(icon: SlateIcon, hovered: bool) -> Option<SlateRaster> {
    match (icon, hovered) {
        (SlateIcon::NavBack, false) => Some(SlateRaster::NavBack),
        (SlateIcon::NavBack, true) => Some(SlateRaster::NavBackHover),
        (SlateIcon::NavForward, false) => Some(SlateRaster::NavForward),
        (SlateIcon::NavForward, true) => Some(SlateRaster::NavForwardHover),
        (SlateIcon::NavRefresh, false) => Some(SlateRaster::NavReload),
        (SlateIcon::NavRefresh, true) => Some(SlateRaster::NavReloadHover),
        _ => None,
    }
}

fn toolbar_menu_icon_center(button_rect: egui::Rect) -> egui::Pos2 {
    button_rect.center() + egui::vec2(TOOLBAR_MENU_ICON_OFFSET_X, 0.0)
}

#[cfg(test)]
fn toolbar_menu_icon_rect(button_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        toolbar_menu_icon_center(button_rect),
        egui::vec2(
            TOOLBAR_MENU_ICON_WIDTH + TOOLBAR_MENU_ICON_STROKE,
            TOOLBAR_MENU_ICON_GAP * 2.0 + TOOLBAR_MENU_ICON_STROKE,
        ),
    )
}

fn footer_load_status_width(available_width: f32) -> f32 {
    available_width.max(0.0)
}

fn footer_load_status_dot_radius() -> f32 {
    FOOTER_LOAD_STATUS_DOT_SIZE / 2.0
}

fn footer_load_status_label_max_chars(status_width: f32) -> usize {
    let label_width =
        (status_width - FOOTER_LOAD_STATUS_DOT_SIZE - FOOTER_LOAD_STATUS_DOT_LABEL_GAP).max(0.0);
    (label_width / (FOOTER_TEXT_SIZE * 0.62)).floor().max(1.0) as usize
}

fn footer_panel_margin() -> egui::Margin {
    egui::Margin {
        left: FOOTER_PANEL_MARGIN_X,
        right: FOOTER_PANEL_MARGIN_X,
        top: FOOTER_PANEL_MARGIN_TOP,
        bottom: FOOTER_PANEL_MARGIN_BOTTOM,
    }
}

fn tab_corner_radius() -> egui::CornerRadius {
    egui::CornerRadius {
        nw: TAB_CORNER_RADIUS,
        ne: TAB_CORNER_RADIUS,
        sw: 0,
        se: 0,
    }
}

#[derive(Clone, Copy, Debug)]
struct ActiveTabSeparatorJoin {
    bridge_rect: egui::Rect,
    separator_y: f32,
    tab_left: f32,
    tab_right: f32,
}

fn active_tab_separator_join(
    strip_rect: egui::Rect,
    active_tab_rect: egui::Rect,
) -> Option<ActiveTabSeparatorJoin> {
    let tab_left = active_tab_rect
        .left()
        .clamp(strip_rect.left(), strip_rect.right());
    let tab_right = active_tab_rect
        .right()
        .clamp(strip_rect.left(), strip_rect.right());
    if tab_right <= tab_left {
        return None;
    }

    let tab_width = tab_right - tab_left;
    let join_inset = ACTIVE_TAB_BOTTOM_JOIN_INSET_X.min(tab_width / 2.0);
    let y = strip_rect.bottom() - 0.5;
    Some(ActiveTabSeparatorJoin {
        bridge_rect: egui::Rect::from_min_max(
            egui::pos2(
                tab_left + join_inset,
                y - ACTIVE_TAB_BOTTOM_JOIN_HEIGHT / 2.0,
            ),
            egui::pos2(
                tab_right - join_inset,
                y + ACTIVE_TAB_BOTTOM_JOIN_HEIGHT / 2.0,
            ),
        ),
        separator_y: y,
        tab_left,
        tab_right,
    })
}

fn extend_active_tab_corner_points(
    points: &mut Vec<egui::Pos2>,
    center: egui::Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
) {
    for step in 1..=ACTIVE_TAB_FILE_CORNER_STEPS {
        let t = step as f32 / ACTIVE_TAB_FILE_CORNER_STEPS as f32;
        let angle = start_angle + (end_angle - start_angle) * t;
        points.push(egui::pos2(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
}

fn active_tab_file_outline_points(
    active_tab_rect: egui::Rect,
    separator_y: f32,
) -> Vec<egui::Pos2> {
    let left = active_tab_rect.left();
    let right = active_tab_rect.right();
    let top = active_tab_rect.top();
    let width = active_tab_rect.width().max(0.0);
    let height = (separator_y - top).max(0.0);
    let radius = f32::from(TAB_CORNER_RADIUS).min(width / 2.0).min(height);

    if radius <= 0.0 {
        return vec![
            egui::pos2(left, separator_y),
            egui::pos2(left, top),
            egui::pos2(right, top),
            egui::pos2(right, separator_y),
        ];
    }

    let mut points = Vec::with_capacity(ACTIVE_TAB_FILE_CORNER_STEPS * 2 + 5);
    points.push(egui::pos2(left, separator_y));
    points.push(egui::pos2(left, top + radius));
    extend_active_tab_corner_points(
        &mut points,
        egui::pos2(left + radius, top + radius),
        radius,
        std::f32::consts::PI,
        std::f32::consts::PI + std::f32::consts::FRAC_PI_2,
    );
    points.push(egui::pos2(right - radius, top));
    extend_active_tab_corner_points(
        &mut points,
        egui::pos2(right - radius, top + radius),
        radius,
        -std::f32::consts::FRAC_PI_2,
        0.0,
    );
    points.push(egui::pos2(right, separator_y));
    points
}

fn inactive_tab_outline_points(tab_rect: egui::Rect) -> Vec<egui::Pos2> {
    active_tab_file_outline_points(tab_rect, tab_rect.bottom() - 0.5)
}

fn draw_inactive_tab_outline(ui: &egui::Ui, tab_rect: egui::Rect) {
    if ui.is_rect_visible(tab_rect) {
        ui.painter().line(
            inactive_tab_outline_points(tab_rect),
            egui::Stroke::new(1.0, inactive_tab_outline_color()),
        );
    }
}

fn push_separator_point(points: &mut Vec<egui::Pos2>, point: egui::Pos2) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

fn active_tab_content_divider_points(
    strip_rect: egui::Rect,
    active_tab_rect: egui::Rect,
) -> Option<(ActiveTabSeparatorJoin, Vec<egui::Pos2>)> {
    let join = active_tab_separator_join(strip_rect, active_tab_rect)?;
    let visible_tab_rect = egui::Rect::from_min_max(
        egui::pos2(join.tab_left, active_tab_rect.top()),
        egui::pos2(join.tab_right, active_tab_rect.bottom()),
    );
    let mut points = Vec::with_capacity(ACTIVE_TAB_FILE_CORNER_STEPS * 2 + 7);

    push_separator_point(&mut points, egui::pos2(strip_rect.left(), join.separator_y));
    push_separator_point(&mut points, egui::pos2(join.tab_left, join.separator_y));
    for point in active_tab_file_outline_points(visible_tab_rect, join.separator_y) {
        push_separator_point(&mut points, point);
    }
    push_separator_point(&mut points, egui::pos2(join.tab_right, join.separator_y));
    push_separator_point(
        &mut points,
        egui::pos2(strip_rect.right(), join.separator_y),
    );

    Some((join, points))
}

fn draw_tab_strip_separator(ui: &egui::Ui, active_tab_rect: Option<egui::Rect>) {
    let strip_rect = ui.max_rect();
    let y = strip_rect.bottom() - 0.5;
    let divider_stroke = egui::Stroke::new(1.0, tab_strip_separator_color());
    let active_outline_stroke = egui::Stroke::new(1.0, active_tab_outline_color());

    if let Some(active_tab_rect) = active_tab_rect
        && let Some((join, _divider_points)) =
            active_tab_content_divider_points(strip_rect, active_tab_rect)
    {
        ui.painter()
            .rect_filled(join.bridge_rect, 0.0, slate_theme::SURFACE);
        ui.painter().line_segment(
            [
                egui::pos2(strip_rect.left(), join.separator_y),
                egui::pos2(join.tab_left, join.separator_y),
            ],
            divider_stroke,
        );
        let visible_tab_rect = egui::Rect::from_min_max(
            egui::pos2(join.tab_left, active_tab_rect.top()),
            egui::pos2(join.tab_right, active_tab_rect.bottom()),
        );
        ui.painter().line(
            active_tab_file_outline_points(visible_tab_rect, join.separator_y),
            active_outline_stroke,
        );
        ui.painter().line_segment(
            [
                egui::pos2(join.tab_right, join.separator_y),
                egui::pos2(strip_rect.right(), join.separator_y),
            ],
            divider_stroke,
        );
        return;
    }

    ui.painter().line_segment(
        [
            egui::pos2(strip_rect.left(), y),
            egui::pos2(strip_rect.right(), y),
        ],
        divider_stroke,
    );
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn load_cjk_fonts(font_candidates: &[(&str, &str)]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut loaded_font_names = Vec::new();

    for (path_str, font_name) in font_candidates.iter() {
        let font_path = Path::new(path_str);
        if font_path.exists() {
            match fs::read(font_path) {
                Ok(bytes) => {
                    if !fonts.font_data.contains_key(*font_name) {
                        fonts
                            .font_data
                            .insert(font_name.to_string(), Arc::new(FontData::from_owned(bytes)));
                        loaded_font_names.push(font_name.to_string());
                        info!("Loaded font: {}", font_name);
                    }
                }
                Err(error) => {
                    info!("Failed to read font {}: {}", font_name, error);
                }
            }
        }
    }

    if !loaded_font_names.is_empty() {
        let proportional = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
        for font_name in loaded_font_names.iter() {
            proportional.insert(0, font_name.clone());
        }
    }

    fonts
}

#[cfg(target_os = "windows")]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (r"C:\Windows\Fonts\malgun.ttf", "Malgun Gothic"), // Korean
        (r"C:\Windows\Fonts\msyh.ttc", "Microsoft YaHei"), // Chinese + Japanese
    ])
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "Noto Sans CJK",
        ), // Ubuntu/Debian
        (
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "Noto Sans CJK",
        ), // Fedora/Arch
        // FreeBSD splits the Noto CJK fonts into regional subsets
        (
            "/usr/local/share/fonts/noto/NotoSansCJKhk-Regular.otf",
            "Noto Sans CJK HK",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKjp-Regular.otf",
            "Noto Sans CJK JP",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKkr-Regular.otf",
            "Noto Sans CJK KR",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKsc-Regular.otf",
            "Noto Sans CJK SC",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKtc-Regular.otf",
            "Noto Sans CJK TC",
        ),
        (
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "WenQuanYi Micro Hei",
        ), // common fallback
        (
            "/usr/local/share/fonts/wqy/wqy-microhei.ttc",
            "WenQuanYi Micro Hei",
        ), // FreeBSD
    ])
}

#[cfg(target_os = "macos")]
fn configure_fonts() -> FontDefinitions {
    // TODO: Default proportional fonts: ["Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"]
    // does not support CJK. Add them for Mac.
    FontDefinitions::default()
}

impl Drop for Gui {
    fn drop(&mut self) {
        self.rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");
        self.context.destroy();
    }
}

impl Gui {
    pub(crate) fn new(
        winit_window: &Window,
        event_loop: &ActiveEventLoop,
        event_loop_proxy: EventLoopProxy<AppEvent>,
        rendering_context: Rc<OffscreenRenderingContext>,
        initial_url: Url,
    ) -> Self {
        rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");
        let mut context = EguiGlow::new(
            event_loop,
            rendering_context.glow_gl_api(),
            None,
            None,
            false,
        );

        let font_definitions = configure_fonts();
        context.egui_ctx.set_fonts(font_definitions);
        slate_theme::apply(&context.egui_ctx);

        context
            .egui_winit
            .init_accesskit(event_loop, winit_window, event_loop_proxy);
        winit_window.set_visible(true);

        context.egui_ctx.options_mut(|options| {
            // Disable the builtin egui handlers for the Ctrl+Plus, Ctrl+Minus and Ctrl+0
            // shortcuts as they don't work well with servoshell's `device-pixel-ratio` CLI argument.
            options.zoom_with_keyboard = false;

            // On platforms where winit fails to obtain a system theme, fall back to a light theme
            // since it is the more common default.
            options.fallback_theme = egui::Theme::Light;
        });

        let (home_favicon_tx, home_favicon_rx) = mpsc::channel();
        Self {
            rendering_context,
            context,
            toolbar_height: Default::default(),
            webview_origin: Point2D::zero(),
            webview_size: Size2D::zero(),
            webview_contains_native_chrome: false,
            location: initial_url.to_string(),
            home_search: String::new(),
            home_bookmarks: default_home_bookmark_cards(),
            home_bookmarks_loaded: false,
            web_history_cards: default_web_history_cards(),
            home_favicon_textures: Default::default(),
            home_favicon_fetches: Default::default(),
            home_favicon_failures: Default::default(),
            home_favicon_tx,
            home_favicon_rx,
            toolbar_menu_popup_id: None,
            location_dirty: false,
            load_status: LoadStatus::Complete,
            status_text: None,
            broadweb_status: BroadwebStatusSnapshot::idle(),
            chrome_element_zoom: CHROME_ELEMENT_ZOOM,
            last_chrome_element_zoom_url: None,
            platform_zoom_factor: Cell::new(1.0),
            can_go_back: false,
            can_go_forward: false,
            favicon_textures: Default::default(),
            slate_icons: Default::default(),
            pending_accesskit_updates: vec![],
        }
    }

    pub(crate) fn has_keyboard_focus(&self) -> bool {
        self.context
            .egui_ctx
            .memory(|memory| memory.focused().is_some())
    }

    pub(crate) fn surrender_focus(&self) {
        self.context.egui_ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    }

    pub(crate) fn on_window_event(
        &mut self,
        winit_window: &Window,
        event: &WindowEvent,
    ) -> EventResponse {
        self.context.on_window_event(winit_window, event)
    }

    /// The height of the top toolbar of this user inteface ie the distance from the top of the
    /// window to the position of the `WebView`.
    pub(crate) fn toolbar_height(&self) -> Length<f32, DeviceIndependentPixel> {
        self.toolbar_height
    }

    pub(crate) fn webview_origin(&self) -> Point2D<f32, DeviceIndependentPixel> {
        self.webview_origin
    }

    pub(crate) fn pixels_per_point(&self) -> f32 {
        self.context.egui_ctx.pixels_per_point()
    }

    /// Return true if the given position should be handled by egui chrome.
    pub(crate) fn is_in_egui_toolbar_rect(
        &self,
        position: Point2D<f32, DeviceIndependentPixel>,
    ) -> bool {
        egui_chrome_captures_mouse_position(
            self.webview_origin,
            self.webview_size,
            self.webview_contains_native_chrome,
            self.is_chrome_popup_open(),
            position,
        )
    }

    fn is_chrome_popup_open(&self) -> bool {
        self.toolbar_menu_popup_id
            .is_some_and(|id| egui::Popup::is_id_open(&self.context.egui_ctx, id))
    }

    fn new_tab_button(ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(NEW_TAB_BUTTON_SIZE), egui::Sense::click());

        if ui.is_rect_visible(rect) {
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, NEW_TAB_BUTTON_RADIUS, slate_theme::PANEL_HOVER);
            }

            let center = rect.center();
            let half = NEW_TAB_ICON_SIZE / 2.0;
            let stroke = egui::Stroke::new(NEW_TAB_ICON_STROKE, new_tab_icon_color());
            ui.painter().line_segment(
                [
                    egui::pos2(center.x - half, center.y),
                    egui::pos2(center.x + half, center.y),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(center.x, center.y - half),
                    egui::pos2(center.x, center.y + half),
                ],
                stroke,
            );
        }

        response
    }

    fn icon_image(texture: egui::load::SizedTexture, size: f32) -> egui::Image<'static> {
        egui::Image::from_texture(texture)
            .fit_to_exact_size(egui::vec2(size, size))
            .bg_fill(egui::Color32::TRANSPARENT)
    }

    fn vertical_separator_with_color(ui: &mut egui::Ui, height: f32, color: egui::Color32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().line_segment(
                [
                    egui::pos2(rect.center().x, rect.top()),
                    egui::pos2(rect.center().x, rect.bottom()),
                ],
                egui::Stroke::new(1.0, color),
            );
        }
    }

    fn vertical_separator(ui: &mut egui::Ui, height: f32) {
        Self::vertical_separator_with_color(ui, height, chrome_vertical_separator_color());
    }

    fn draw_footer_top_separator(ctx: &egui::Context, footer_rect: egui::Rect) {
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("footer_top")));
        let y = footer_rect.top() + 0.5;
        painter.line_segment(
            [
                egui::pos2(footer_rect.left(), y),
                egui::pos2(footer_rect.right(), y),
            ],
            egui::Stroke::new(1.0, footer_top_separator_color()),
        );
    }

    fn toolbar_navigation_button(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        icon: SlateIcon,
        enabled: bool,
    ) -> egui::Response {
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(TOOLBAR_BUTTON_SIZE), sense);
        if ui.is_rect_visible(rect) {
            if enabled && response.hovered() {
                ui.painter()
                    .rect_filled(rect, TOOLBAR_BUTTON_RADIUS, slate_theme::PANEL_HOVER);
            }
            let hovered = enabled && response.hovered();
            if let Some(raster) = toolbar_navigation_raster(icon, hovered) {
                let texture = slate_icons.raster_mask_texture(
                    ui.ctx(),
                    raster,
                    toolbar_navigation_icon_color(enabled),
                );
                ui.painter().image(
                    texture.id,
                    toolbar_navigation_icon_rect(rect, icon),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }
        response
    }

    fn toolbar_hover_raster_button(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        icon: SlateRaster,
        hover_icon: SlateRaster,
        enabled: bool,
    ) -> egui::Response {
        let icon_texture =
            slate_icons.raster_mask_texture(ui.ctx(), icon, toolbar_navigation_icon_color(enabled));
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(TOOLBAR_BUTTON_SIZE), sense);
        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            if hovered {
                ui.painter()
                    .rect_filled(rect, TOOLBAR_BUTTON_RADIUS, slate_theme::PANEL_HOVER);
            }
            let texture = if hovered {
                slate_icons.raster_mask_texture(
                    ui.ctx(),
                    hover_icon,
                    toolbar_navigation_icon_color(true),
                )
            } else {
                icon_texture
            };
            let icon_rect =
                egui::Rect::from_center_size(rect.center(), Vec2::splat(TOOLBAR_ICON_SIZE));
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        response
    }

    fn toolbar_icon_button_sized(
        ui: &mut egui::Ui,
        texture: egui::load::SizedTexture,
        icon_size: f32,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(TOOLBAR_BUTTON_SIZE), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, TOOLBAR_BUTTON_RADIUS, slate_theme::PANEL_HOVER);
            }
            let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        response
    }

    fn toolbar_menu_button(ui: &mut egui::Ui, selected: bool) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(TOOLBAR_BUTTON_SIZE), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let fill = if selected {
                slate_theme::PANEL
            } else if response.hovered() {
                slate_theme::PANEL_HOVER
            } else {
                egui::Color32::TRANSPARENT
            };
            if fill != egui::Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, TOOLBAR_BUTTON_RADIUS, fill);
            }

            let center = toolbar_menu_icon_center(rect);
            for offset in [-TOOLBAR_MENU_ICON_GAP, 0.0, TOOLBAR_MENU_ICON_GAP] {
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - TOOLBAR_MENU_ICON_WIDTH / 2.0, center.y + offset),
                        egui::pos2(center.x + TOOLBAR_MENU_ICON_WIDTH / 2.0, center.y + offset),
                    ],
                    egui::Stroke::new(TOOLBAR_MENU_ICON_STROKE, toolbar_menu_icon_color(selected)),
                );
            }
        }
        response
    }

    fn draw_toolbar_menu(
        menu_button: &egui::Response,
        state: &RunningAppState,
        window: &ServoShellWindow,
        location_dirty: &mut bool,
        toolbar_menu_popup_id: &mut Option<egui::Id>,
    ) {
        *toolbar_menu_popup_id = Some(egui::Popup::default_response_id(menu_button));
        egui::Popup::menu(menu_button)
            .align(egui::RectAlign::BOTTOM_END)
            .width(260.0)
            .show(|ui| {
                ui.set_min_width(240.0);

                if ui.button("New Tab").clicked() {
                    *location_dirty = false;
                    window.queue_user_interface_command(UserInterfaceCommand::NewWebView);
                    ui.close();
                }

                if ui.button("Reload Page").clicked() {
                    *location_dirty = false;
                    window.queue_user_interface_command(UserInterfaceCommand::Reload);
                    ui.close();
                }

                ui.separator();

                let mut experimental_preferences_enabled = state.experimental_preferences_enabled();
                if ui
                    .checkbox(
                        &mut experimental_preferences_enabled,
                        "Enable experimental prefs",
                    )
                    .clicked()
                {
                    state.set_experimental_preferences_enabled(experimental_preferences_enabled);
                    *location_dirty = false;
                    window.queue_user_interface_command(UserInterfaceCommand::ReloadAll);
                    ui.close();
                }

                ui.separator();

                ui.add_enabled(false, egui::Button::new("Bookmarks"));
                if ui.button("Downloads").clicked() {
                    *location_dirty = false;
                    window.queue_user_interface_command(UserInterfaceCommand::Go(
                        "slate://downloads".to_string(),
                    ));
                    ui.close();
                }
                ui.add_enabled(false, egui::Button::new("History"));
                if ui.button("Settings").clicked() {
                    *location_dirty = false;
                    window.queue_user_interface_command(UserInterfaceCommand::Go(
                        "slate://settings".to_string(),
                    ));
                    ui.close();
                }
            });
    }

    fn tab_close_button(ui: &mut egui::Ui, texture: egui::load::SizedTexture) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(TAB_CLOSE_BUTTON_SIZE), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, TAB_CLOSE_BUTTON_RADIUS, slate_theme::PANEL_HOVER);
            }
            let icon_rect =
                egui::Rect::from_center_size(rect.center(), Vec2::splat(TAB_CLOSE_ICON_SIZE));
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        response.widget_info(|| {
            let mut info = WidgetInfo::new(WidgetType::Button);
            info.label = Some("Close".into());
            info
        });
        response.on_hover_text("Close")
    }

    fn tab_title_button(
        ui: &mut egui::Ui,
        label: &str,
        active: bool,
        content_width: f32,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(tab_title_width(content_width), TAB_CONTENT_HEIGHT),
            egui::Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            ui.painter().text(
                egui::pos2(rect.left(), rect.center().y),
                egui::Align2::LEFT_CENTER,
                truncate_with_ellipsis(label, 20),
                egui::FontId::proportional(TAB_TITLE_TEXT_SIZE),
                tab_title_color(active),
            );
        }

        response.widget_info(|| {
            let mut info = WidgetInfo::new(WidgetType::Button);
            info.label = Some(label.into());
            info.selected = Some(active);
            info
        });
        response.on_hover_ui(|ui| {
            ui.label(label);
        })
    }

    fn address_raster_button_sized(
        ui: &mut egui::Ui,
        texture: egui::load::SizedTexture,
        icon_size: f32,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(ADDRESS_BOOKMARK_BUTTON_SIZE),
            egui::Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            if response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    ADDRESS_BOOKMARK_BUTTON_RADIUS,
                    slate_theme::PANEL_HOVER,
                );
            }
            let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        response
    }

    fn fallback_tab_icon(index: usize) -> SlateIcon {
        match index {
            1 => SlateIcon::TabResearch,
            2 => SlateIcon::TabCalendar,
            _ => SlateIcon::TabWeb,
        }
    }

    fn native_chrome_page_for_url(url: &Url) -> Option<NativeChromePage> {
        if is_slate_home_url(url) {
            Some(NativeChromePage::Home)
        } else if is_slate_web_url(url) {
            Some(NativeChromePage::Web)
        } else {
            None
        }
    }

    fn active_native_chrome_page(window: &ServoShellWindow) -> Option<NativeChromePage> {
        window
            .active_webview()
            .and_then(|webview| webview.url())
            .as_ref()
            .and_then(Self::native_chrome_page_for_url)
    }

    fn active_webview_is_blank(window: &ServoShellWindow) -> bool {
        window
            .active_webview()
            .and_then(|webview| webview.url())
            .as_ref()
            .is_some_and(is_slate_blank_url)
    }

    fn rail_icon_button(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        icon: SlateIcon,
        selected: bool,
        label: &str,
    ) -> egui::Response {
        let texture = slate_icons.texture(ui.ctx(), icon, rail_icon_color(selected));

        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(RAIL_BUTTON_SIZE), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let fill = rail_button_fill(selected, response.hovered());
            if fill != egui::Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, RAIL_BUTTON_RADIUS, fill);
            }
            let icon_center = egui::pos2(
                rect.center().x,
                rect.center().y - (RAIL_LABEL_TEXT_SIZE + RAIL_ICON_LABEL_GAP) / 2.0,
            );
            let icon_rect = egui::Rect::from_center_size(icon_center, Vec2::splat(RAIL_ICON_SIZE));
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            ui.painter().text(
                egui::pos2(
                    rect.center().x,
                    icon_rect.bottom() + RAIL_ICON_LABEL_GAP + RAIL_LABEL_TEXT_SIZE / 2.0,
                ),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(RAIL_LABEL_TEXT_SIZE),
                rail_icon_color(selected),
            );
        }

        response.widget_info(|| {
            let mut info = WidgetInfo::new(WidgetType::Button);
            info.label = Some(label.into());
            info.selected = Some(selected);
            info
        });
        response.on_hover_text(label)
    }

    fn draw_app_rail(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        active_page: Option<RailPage>,
    ) -> (bool, bool, bool) {
        let mut home_clicked = false;
        let mut web_clicked = false;
        let mut downloads_clicked = false;
        ui.vertical_centered(|ui| {
            ui.add_space(RAIL_TOP_SPACE);
            let home_button = Self::rail_icon_button(
                ui,
                slate_icons,
                SlateIcon::AppHome,
                active_page == Some(RailPage::Home),
                "Home",
            );
            if home_button.clicked() {
                home_clicked = true;
            }
            ui.add_space(RAIL_ITEM_GAP);
            let web_button = Self::rail_icon_button(
                ui,
                slate_icons,
                SlateIcon::AppWeb,
                active_page == Some(RailPage::Web),
                "Web",
            );
            if web_button.clicked() {
                web_clicked = true;
            }
            ui.add_space(RAIL_ITEM_GAP);
            let downloads_button = Self::rail_icon_button(
                ui,
                slate_icons,
                SlateIcon::AppDownloads,
                active_page == Some(RailPage::Downloads),
                "Downloads",
            );
            if downloads_button.clicked() {
                downloads_clicked = true;
            }
            ui.add_space(RAIL_ITEM_GAP);
            Self::rail_icon_button(ui, slate_icons, SlateIcon::AppCalendar, false, "Calendar");
            ui.add_space(RAIL_ITEM_GAP);
            Self::rail_icon_button(ui, slate_icons, SlateIcon::AppMessaging, false, "Messages");
        });
        (home_clicked, web_clicked, downloads_clicked)
    }

    fn draw_interactive_app_rail(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        window: &ServoShellWindow,
        location_dirty: &mut bool,
        location: &str,
    ) {
        let active_page = if location_is_home(location) {
            Some(RailPage::Home)
        } else if location_is_web(location) {
            Some(RailPage::Web)
        } else if location_is_downloads(location) {
            Some(RailPage::Downloads)
        } else {
            None
        };
        let (home_clicked, web_clicked, downloads_clicked) =
            Self::draw_app_rail(ui, slate_icons, active_page);
        if home_clicked {
            *location_dirty = false;
            window
                .queue_user_interface_command(UserInterfaceCommand::Go("slate://home".to_string()));
        }
        if web_clicked {
            *location_dirty = false;
            window
                .queue_user_interface_command(UserInterfaceCommand::Go("slate://web".to_string()));
        }
        if downloads_clicked {
            *location_dirty = false;
            window.queue_user_interface_command(UserInterfaceCommand::Go(
                "slate://downloads".to_string(),
            ));
        }
    }

    fn draw_app_title(ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(APP_TITLE_WIDTH, APP_TITLE_HEIGHT),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, 0.0, app_title_background_color());
        ui.painter().line_segment(
            [rect.right_top(), rect.right_bottom()],
            egui::Stroke::new(1.0, slate_theme::BORDER),
        );
        ui.painter().text(
            egui::pos2(rect.min.x + APP_TITLE_LEFT_PADDING, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Slate",
            egui::FontId::proportional(APP_TITLE_TEXT_SIZE),
            app_title_text_color(),
        );
    }

    fn footer_load_status_label(
        load_status: LoadStatus,
        broadweb_status: &BroadwebStatusSnapshot,
        location: &str,
    ) -> String {
        if location_has_broadweb_status(location)
            && matches!(
                broadweb_status.kind,
                BroadwebStatusKind::Fetching
                    | BroadwebStatusKind::SwitchingGateway
                    | BroadwebStatusKind::Complete
                    | BroadwebStatusKind::Error
            )
        {
            return broadweb_status.message.clone();
        }

        match load_status {
            LoadStatus::Started => "Loading...".to_string(),
            LoadStatus::HeadParsed => "Rendering...".to_string(),
            LoadStatus::Complete => "Ready".to_string(),
        }
    }

    fn draw_footer_load_status(
        ui: &mut egui::Ui,
        status_width: f32,
        load_status: LoadStatus,
        broadweb_status: &BroadwebStatusSnapshot,
        location: &str,
    ) {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(
                footer_load_status_width(status_width),
                FOOTER_LOAD_STATUS_HEIGHT,
            ),
            egui::Sense::hover(),
        );
        let label = Self::footer_load_status_label(load_status, broadweb_status, location);
        let time_seconds = ui.input(|input| input.time);

        if ui.is_rect_visible(rect) {
            let dot_center = egui::pos2(
                rect.left() + FOOTER_LOAD_STATUS_DOT_SIZE / 2.0,
                rect.center().y,
            );
            ui.painter().circle_filled(
                dot_center,
                footer_load_status_dot_radius(),
                footer_load_status_indicator_color_at(load_status, broadweb_status, time_seconds),
            );
            ui.painter().text(
                egui::pos2(
                    dot_center.x
                        + FOOTER_LOAD_STATUS_DOT_SIZE / 2.0
                        + FOOTER_LOAD_STATUS_DOT_LABEL_GAP,
                    rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                truncate_with_ellipsis(&label, footer_load_status_label_max_chars(rect.width())),
                egui::FontId::proportional(FOOTER_TEXT_SIZE),
                footer_status_text_color(),
            );
        }

        response.on_hover_text(label);
    }

    fn draw_footer(
        ui: &mut egui::Ui,
        load_status: LoadStatus,
        broadweb_status: &BroadwebStatusSnapshot,
        location: &str,
    ) {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        ui.horizontal_centered(|ui| {
            ui.add_space(FOOTER_LEFT_PADDING);
            let status_width = (ui.available_width() - FOOTER_RIGHT_PADDING).max(0.0);
            Self::draw_footer_load_status(ui, status_width, load_status, broadweb_status, location);
        });
    }

    fn draw_home_bookmark_card(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        favicon_textures: &HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
        width: f32,
        bookmark: &HomeBookmarkCard,
    ) -> (egui::Rect, egui::Response) {
        let frame = egui::Frame::NONE
            .fill(home_metric_card_background_color())
            .stroke(egui::Stroke::new(1.0, slate_theme::BORDER))
            .corner_radius(8)
            .shadow(home_panel_shadow())
            .inner_margin(egui::Margin::symmetric(
                HOME_METRIC_CARD_INNER_MARGIN_X,
                HOME_METRIC_CARD_INNER_MARGIN_Y,
            ))
            .show(ui, |ui| {
                let content_size = egui::vec2(
                    home_metric_card_content_width(width),
                    home_metric_card_content_height(),
                );
                let (content_rect, _) = ui.allocate_exact_size(content_size, egui::Sense::hover());
                if ui.is_rect_visible(content_rect) {
                    let active = bookmark.url.is_some();
                    let icon_rect = egui::Rect::from_center_size(
                        egui::pos2(content_rect.center().x, content_rect.top() + 26.0),
                        egui::Vec2::splat(HOME_METRIC_ICON_SIZE),
                    );
                    if let Some((_, texture)) = bookmark
                        .favicon_key
                        .as_ref()
                        .and_then(|key| favicon_textures.get(key))
                    {
                        ui.painter().image(
                            texture.id,
                            icon_rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    } else {
                        let texture = slate_icons.texture(
                            ui.ctx(),
                            SlateIcon::HomeHeroShield,
                            slate_theme::MUTED,
                        );
                        ui.painter().image(
                            texture.id,
                            icon_rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }

                    let label = truncate_with_ellipsis(&bookmark.label, 22);
                    ui.painter().text(
                        egui::pos2(
                            content_rect.center().x,
                            icon_rect.bottom() + HOME_METRIC_ICON_LABEL_GAP + 8.0,
                        ),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(HOME_METRIC_LABEL_TEXT_SIZE),
                        if active {
                            slate_theme::TEXT
                        } else {
                            slate_theme::MUTED
                        },
                    );

                    let detail = truncate_with_ellipsis(&bookmark.detail, 24);
                    ui.painter().text(
                        egui::pos2(
                            content_rect.center().x,
                            icon_rect.bottom()
                                + HOME_METRIC_ICON_LABEL_GAP
                                + HOME_METRIC_LABEL_TEXT_SIZE
                                + HOME_METRIC_DETAIL_GAP
                                + 14.0,
                        ),
                        egui::Align2::CENTER_CENTER,
                        detail,
                        egui::FontId::proportional(HOME_METRIC_DETAIL_TEXT_SIZE),
                        home_metric_detail_color(),
                    );
                }
            });

        let response = ui.interact(
            frame.response.rect,
            ui.make_persistent_id(("home_bookmark", &bookmark.label)),
            if bookmark.url.is_some() {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        if ui.is_rect_visible(response.rect) && response.hovered() && bookmark.url.is_some() {
            ui.painter().rect_stroke(
                response.rect,
                8,
                egui::Stroke::new(1.0, slate_theme::TEAL),
                egui::StrokeKind::Outside,
            );
        }

        response.widget_info(|| {
            let mut info = WidgetInfo::new(if bookmark.url.is_some() {
                WidgetType::Button
            } else {
                WidgetType::Label
            });
            info.label = Some(bookmark.label.clone());
            info
        });
        let response = response.on_hover_ui(|ui| {
            ui.label(&bookmark.label);
            if let Some(url) = &bookmark.url {
                ui.label(url);
            }
        });

        (frame.response.rect, response)
    }

    fn draw_home_metrics(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        favicon_textures: &HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
        bookmarks: &[HomeBookmarkCard],
    ) -> (egui::Rect, Option<String>) {
        let layout = home_metrics_layout(ui.available_width());
        let mut bounds = None;
        let mut navigation_request = None;
        for (row_index, row) in bookmarks.chunks(layout.columns).enumerate() {
            if row_index > 0 {
                ui.add_space(layout.spacing);
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                for (column_index, bookmark) in row.iter().enumerate() {
                    if column_index > 0 {
                        ui.add_space(layout.spacing);
                    }
                    let (card_rect, response) = Self::draw_home_bookmark_card(
                        ui,
                        slate_icons,
                        favicon_textures,
                        layout.card_width,
                        bookmark,
                    );
                    if response.clicked()
                        && let Some(url) = &bookmark.url
                    {
                        navigation_request = Some(url.clone());
                    }
                    bounds =
                        Some(bounds.map_or(card_rect, |rect: egui::Rect| rect.union(card_rect)));
                }
            });
        }

        (bounds.unwrap_or(egui::Rect::NOTHING), navigation_request)
    }

    fn draw_home_content(
        ui: &mut egui::Ui,
        home_rect: egui::Rect,
        slate_icons: &mut SlateIconCache,
        favicon_textures: &HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
        home_search: &mut String,
        home_bookmarks: &[HomeBookmarkCard],
    ) -> HomeContentResponse {
        let mut layout = HomeContentLayout::default();
        let mut navigation_request = None;

        ui.add_space(home_top_space(home_rect.height()));
        ui.vertical(|ui| {
            let available_width = ui.available_width();
            ui.horizontal(|ui| {
                ui.add_space(home_hero_left_space(available_width, HOME_HERO_SIZE));
                let hero =
                    slate_icons.texture(ui.ctx(), SlateIcon::HomeHeroShield, slate_theme::TEAL);
                let hero_response = ui.add(
                    egui::Image::from_texture(hero)
                        .fit_to_exact_size(egui::vec2(HOME_HERO_SIZE, HOME_HERO_SIZE)),
                );
                layout.hero_rect = hero_response.rect;
            });
            ui.add_space(HOME_HERO_MOTTO_GAP);
            let available_width = ui.available_width();
            ui.horizontal(|ui| {
                ui.add_space(home_content_left_space(available_width, HOME_MOTTO_WIDTH));
                let (motto_rect, response) = ui.allocate_exact_size(
                    egui::vec2(HOME_MOTTO_WIDTH, HOME_MOTTO_HEIGHT),
                    egui::Sense::hover(),
                );
                layout.motto_rect = motto_rect;
                if ui.is_rect_visible(motto_rect) {
                    ui.painter().text(
                        motto_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        SLATE_MOTTO,
                        egui::FontId::proportional(HOME_MOTTO_TEXT_SIZE),
                        slate_theme::TEXT,
                    );
                }
                let enabled = ui.is_enabled();
                response.widget_info(move || {
                    WidgetInfo::labeled(WidgetType::Label, enabled, SLATE_MOTTO)
                });
            });
            ui.add_space(HOME_HERO_TO_SEARCH_GAP);

            let available_width = ui.available_width();
            let search_width = home_search_width(available_width);
            let search_content_width = home_search_content_width(search_width);
            let home_search_id = egui::Id::new("home_search_input");
            let mut search_response = None;
            ui.horizontal(|ui| {
                ui.add_space(home_content_left_space(available_width, search_width));
                let search_frame_response = egui::Frame::NONE
                    .fill(home_search_background_color())
                    .stroke(egui::Stroke::new(1.0, home_search_border_color()))
                    .corner_radius(HOME_SEARCH_CORNER_RADIUS)
                    .shadow(home_panel_shadow())
                    .inner_margin(egui::Margin::symmetric(HOME_SEARCH_INNER_MARGIN_X, 0))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(search_content_width, HOME_SEARCH_HEIGHT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let search_icon = slate_icons.texture(
                                    ui.ctx(),
                                    SlateIcon::HomeSearch,
                                    home_search_icon_color(),
                                );
                                let (slot_rect, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(HOME_SEARCH_ICON_SIZE),
                                    egui::Sense::hover(),
                                );
                                let icon_rect = home_search_icon_rect(slot_rect);
                                layout.search_icon_rect = icon_rect;
                                if ui.is_rect_visible(slot_rect) {
                                    ui.painter().image(
                                        search_icon.id,
                                        icon_rect,
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                                ui.add_space(HOME_SEARCH_ICON_GAP);
                                ui.add_sized(
                                    [ui.available_width(), HOME_SEARCH_TEXT_HEIGHT],
                                    egui::TextEdit::singleline(home_search)
                                        .id(home_search_id)
                                        .font(egui::FontId::proportional(
                                            HOME_SEARCH_INPUT_TEXT_SIZE,
                                        ))
                                        .frame(egui::Frame::NONE)
                                        .hint_text("Search the web or enter an address"),
                                )
                            },
                        )
                        .inner
                    });
                search_response = Some(search_frame_response.inner);
                layout.search_rect = search_frame_response.response.rect;
            });
            let search_response = search_response.expect("home search should be rendered");

            if search_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let request = home_search.trim().to_owned();
                if !request.is_empty() {
                    navigation_request = Some(request);
                    home_search.clear();
                }
            }

            ui.add_space(HOME_SEARCH_TO_METRICS_GAP);
            let available_width = ui.available_width();
            let metrics_height = ui.available_height().max(0.0);
            let metrics_width = home_metrics_row_width(home_metrics_layout(home_rect.width()));
            ui.horizontal(|ui| {
                ui.add_space(home_content_left_space(available_width, metrics_width));
                let metrics_response = ui.allocate_ui_with_layout(
                    egui::vec2(metrics_width, metrics_height),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| Self::draw_home_metrics(ui, slate_icons, favicon_textures, home_bookmarks),
                );
                layout.metrics_rect = metrics_response.inner.0;
                if let Some(request) = metrics_response.inner.1 {
                    navigation_request = Some(request);
                }
            });
        });

        HomeContentResponse {
            navigation_request,
            layout,
        }
    }

    fn draw_home_view(
        ctx: &egui::Context,
        available_rect: egui::Rect,
        slate_icons: &mut SlateIconCache,
        favicon_textures: &HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
        home_search: &mut String,
        home_bookmarks: &[HomeBookmarkCard],
        window: &ServoShellWindow,
    ) {
        egui::Area::new(Id::new("slate_home_view"))
            .order(Order::Foreground)
            .fixed_pos(available_rect.min)
            .show(ctx, |ui| {
                let home_rect = egui::Rect::from_min_size(ui.min_rect().min, available_rect.size());
                ui.set_min_size(home_rect.size());
                ui.painter()
                    .rect_filled(home_rect, 0.0, home_view_background_color());
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(home_rect), |ui| {
                        Self::draw_home_content(
                            ui,
                            home_rect,
                            slate_icons,
                            favicon_textures,
                            home_search,
                            home_bookmarks,
                        )
                    })
                    .inner;
                let _ = response.layout;
                if let Some(request) = response.navigation_request {
                    window.queue_user_interface_command(UserInterfaceCommand::Go(request));
                }
            });
    }

    fn draw_status_text(ctx: &egui::Context, available_rect: egui::Rect, status_text: &str) {
        let available_width = available_rect.width() - STATUS_BUBBLE_MARGIN_X * 2.0;
        let bubble_width = status_bubble_width(status_text, available_width);
        if bubble_width <= 0.0 {
            return;
        }

        let rect = egui::Rect::from_min_size(
            egui::pos2(
                available_rect.min.x + STATUS_BUBBLE_MARGIN_X,
                available_rect.max.y - STATUS_BUBBLE_MARGIN_Y - STATUS_BUBBLE_HEIGHT,
            ),
            egui::vec2(bubble_width, STATUS_BUBBLE_HEIGHT),
        );
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("status_text")));
        let shadow_rect = rect.translate(egui::vec2(0.0, 1.0));
        painter.rect_filled(
            shadow_rect,
            STATUS_BUBBLE_CORNER_RADIUS,
            egui::Color32::from_black_alpha(STATUS_BUBBLE_SHADOW_ALPHA),
        );
        painter.rect_filled(rect, STATUS_BUBBLE_CORNER_RADIUS, slate_theme::SURFACE);
        painter.rect_stroke(
            rect,
            STATUS_BUBBLE_CORNER_RADIUS,
            egui::Stroke::new(1.0, slate_theme::BORDER),
            egui::StrokeKind::Outside,
        );
        painter.text(
            egui::pos2(
                rect.left() + STATUS_BUBBLE_HORIZONTAL_PADDING,
                rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            status_bubble_label(status_text, bubble_width),
            egui::FontId::proportional(STATUS_TEXT_SIZE),
            slate_theme::MUTED,
        );
    }

    /// Draws a browser tab, checking for clicks and queues appropriate [`UserInterfaceCommand`]s.
    fn browser_tab(
        ui: &mut egui::Ui,
        window: &ServoShellWindow,
        webview: WebView,
        favicon_texture: Option<egui::load::SizedTexture>,
        fallback_icon: egui::load::SizedTexture,
        active_close_icon: egui::load::SizedTexture,
        inactive_close_icon: egui::load::SizedTexture,
        tab_width: f32,
    ) -> egui::Rect {
        let label = match (webview.page_title(), webview.url()) {
            (_, Some(url)) if is_slate_home_url(&url) => "Home".into(),
            (_, Some(url)) if is_slate_web_url(&url) => "Web".into(),
            (_, Some(url)) if is_slate_blank_url(&url) => "New Tab".into(),
            (Some(title), _) if !title.is_empty() => title,
            (_, Some(url)) => url.to_string(),
            _ => "New Tab".into(),
        };

        let inactive_bg_color = inactive_tab_background_color();
        let inactive_hover_bg_color = inactive_tab_hover_background_color();
        let active_bg_color = active_tab_background_color();
        let active = window.active_webview().map(|webview| webview.id()) == Some(webview.id());
        let tab_content_bg_color = if active {
            active_bg_color
        } else {
            inactive_bg_color
        };
        let tab_content_hover_bg_color = if active {
            active_bg_color
        } else {
            inactive_hover_bg_color
        };
        let tab_content_width = tab_content_width(tab_width);

        // Setup a tab frame that will contain the favicon, title and close button
        let mut tab_frame = egui::Frame::NONE
            .fill(tab_content_bg_color)
            .stroke(egui::Stroke::NONE)
            .corner_radius(tab_corner_radius())
            .inner_margin(egui::Margin::symmetric(
                TAB_INNER_MARGIN_X,
                TAB_INNER_MARGIN_Y,
            ))
            .begin(ui);
        {
            tab_frame.content_ui.set_width(tab_content_width);
            tab_frame.content_ui.set_min_height(TAB_CONTENT_HEIGHT);

            let visuals = tab_frame.content_ui.visuals_mut();
            // Remove the stroke so we don't see the border between the close button and the label
            visuals.widgets.active.bg_stroke.width = 0.0;
            visuals.widgets.hovered.bg_stroke.width = 0.0;
            // Now we make sure the fill color is always the same, irrespective of state, that way
            // we can make sure that both the label and close button have the same background color
            visuals.widgets.noninteractive.weak_bg_fill = tab_content_bg_color;
            visuals.widgets.inactive.weak_bg_fill = tab_content_bg_color;
            visuals.widgets.hovered.weak_bg_fill = tab_content_hover_bg_color;
            visuals.widgets.active.weak_bg_fill = tab_content_hover_bg_color;
            visuals.selection.bg_fill = active_bg_color;
            visuals.selection.stroke.color = visuals.widgets.active.fg_stroke.color;
            visuals.widgets.hovered.fg_stroke.color = visuals.widgets.active.fg_stroke.color;

            // Expansion would also show that they are 2 separate widgets
            visuals.widgets.active.expansion = 0.0;
            visuals.widgets.hovered.expansion = 0.0;

            let icon = favicon_texture.unwrap_or(fallback_icon);
            let mut should_close = false;
            let mut should_activate = false;
            tab_frame.content_ui.allocate_ui_with_layout(
                egui::vec2(tab_content_width, TAB_CONTENT_HEIGHT),
                egui::Layout::left_to_right(TAB_CONTENT_ALIGN),
                |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    ui.add(Self::icon_image(icon, TAB_ICON_SIZE));
                    ui.add_space(TAB_ICON_TITLE_GAP);

                    let tab = Self::tab_title_button(ui, &label, active, tab_content_width);
                    ui.add_space(TAB_TITLE_CLOSE_GAP);

                    let close_icon = if active {
                        active_close_icon
                    } else {
                        inactive_close_icon
                    };
                    let close_button = Self::tab_close_button(ui, close_icon);
                    should_close = close_button.clicked()
                        || close_button.middle_clicked()
                        || tab.middle_clicked();
                    should_activate = !active && tab.clicked();
                },
            );

            if should_close {
                window
                    .queue_user_interface_command(UserInterfaceCommand::CloseWebView(webview.id()));
            } else if should_activate {
                window.activate_webview(webview.id());
            }
        }

        let response = tab_frame.allocate_space(ui);
        let fill_color = if active {
            active_bg_color
        } else if response.hovered() {
            inactive_hover_bg_color
        } else {
            inactive_bg_color
        };
        tab_frame.frame.fill = fill_color;
        tab_frame.end(ui);
        if !active {
            draw_inactive_tab_outline(ui, response.rect);
        }
        response.rect
    }

    /// Update the user interface, but do not paint the updated state.
    pub(crate) fn update(
        &mut self,
        state: &RunningAppState,
        window: &ServoShellWindow,
        headed_window: &headed_window::HeadedWindow,
    ) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        self.update_broadweb_status();
        self.update_chrome_element_zoom(window);
        let active_native_chrome_page = Self::active_native_chrome_page(window);
        let active_webview_is_blank = Self::active_webview_is_blank(window);
        let _ = self.update_home_bookmarks(&state.profile_database);
        if active_native_chrome_page == Some(NativeChromePage::Web) {
            let _ = self.update_web_history(&state.profile_database);
        }
        let effective_egui_zoom_factor = self.effective_egui_zoom_factor();
        let Self {
            rendering_context,
            context,
            toolbar_height,
            webview_origin,
            webview_size,
            webview_contains_native_chrome,
            location,
            home_search,
            home_bookmarks,
            web_history_cards,
            home_favicon_textures,
            home_favicon_fetches,
            home_favicon_failures,
            home_favicon_tx,
            home_favicon_rx,
            toolbar_menu_popup_id,
            location_dirty,
            load_status,
            broadweb_status,
            favicon_textures,
            slate_icons,
            ..
        } = self;

        let winit_window = headed_window.winit_window();
        context.run(winit_window, |ctx| {
            slate_theme::apply(ctx);
            ctx.set_zoom_factor(effective_egui_zoom_factor);
            load_pending_favicons(ctx, window, favicon_textures);
            *webview_contains_native_chrome = active_native_chrome_page.is_some();
            if let Some(active_cards) = match active_native_chrome_page {
                Some(NativeChromePage::Home) => Some(home_bookmarks.as_slice()),
                Some(NativeChromePage::Web) => Some(web_history_cards.as_slice()),
                None => None,
            } && Self::update_home_bookmark_favicons(
                ctx,
                &state.profile_database,
                active_cards,
                home_favicon_textures,
                home_favicon_fetches,
                home_favicon_failures,
                home_favicon_rx,
                home_favicon_tx,
            ) {
                ctx.request_repaint();
            }

            // TODO: While in fullscreen add some way to mitigate the increased phishing risk
            // when not displaying the URL bar: https://github.com/servo/servo/issues/32443
            if winit_window.fullscreen().is_none() {
                let tabs_frame = egui::Frame::NONE
                    .fill(tab_strip_background_color())
                    .inner_margin(egui::Margin::symmetric(0, 0));
                Panel::top("tabs")
                    .exact_size(TAB_STRIP_HEIGHT)
                    .frame(tabs_frame)
                    .show_separator_line(false)
                    .show_inside(ctx, |ui| {
                        let mut active_tab_rect = None;
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                Self::draw_app_title(ui);
                                let tab_strip_available_width = ui.available_width();

                                egui::ScrollArea::horizontal()
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                    )
                                    .show(ui, |ui| {
                                        ui.allocate_ui_with_layout(
                                            ui.available_size(),
                                            egui::Layout::left_to_right(TAB_STRIP_CONTENT_ALIGN),
                                            |ui| {
                                                ui.spacing_mut().item_spacing =
                                                    egui::vec2(0.0, 0.0);
                                                let webviews = window.webviews();
                                                let tab_width = tab_width_for_strip(
                                                    tab_strip_available_width,
                                                    webviews.len(),
                                                );
                                                for (index, (id, webview)) in
                                                    webviews.into_iter().enumerate()
                                                {
                                                    let favicon = favicon_textures
                                                        .get(&id)
                                                        .map(|(_, favicon)| favicon)
                                                        .copied();
                                                    let active = window
                                                        .active_webview()
                                                        .map(|webview| webview.id())
                                                        == Some(id);
                                                    let fallback_icon_color =
                                                        tab_icon_color(active);
                                                    let fallback_icon = slate_icons.texture(
                                                        ui.ctx(),
                                                        Self::fallback_tab_icon(index),
                                                        fallback_icon_color,
                                                    );
                                                    let close_icon = slate_icons
                                                        .raster_mask_texture(
                                                            ui.ctx(),
                                                            tab_close_raster(true),
                                                            tab_close_icon_color(true),
                                                        );
                                                    let inactive_close_icon = slate_icons
                                                        .raster_mask_texture(
                                                            ui.ctx(),
                                                            tab_close_raster(false),
                                                            tab_close_icon_color(false),
                                                        );
                                                    let tab_rect = Self::browser_tab(
                                                        ui,
                                                        window,
                                                        webview,
                                                        favicon,
                                                        fallback_icon,
                                                        close_icon,
                                                        inactive_close_icon,
                                                        tab_width,
                                                    );
                                                    if active {
                                                        active_tab_rect = Some(tab_rect);
                                                    }
                                                }

                                                ui.add_space(NEW_TAB_LEFT_GAP);
                                                let new_tab_button = ui
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(
                                                            NEW_TAB_BUTTON_SIZE,
                                                            NEW_TAB_SLOT_HEIGHT,
                                                        ),
                                                        egui::Layout::left_to_right(
                                                            TAB_CONTENT_ALIGN,
                                                        ),
                                                        Gui::new_tab_button,
                                                    )
                                                    .inner;
                                                new_tab_button.widget_info(|| {
                                                    let mut info =
                                                        WidgetInfo::new(WidgetType::Button);
                                                    info.label = Some("New tab".into());
                                                    info
                                                });
                                                if new_tab_button.clicked() {
                                                    window.queue_user_interface_command(
                                                        UserInterfaceCommand::NewWebView,
                                                    );
                                                }
                                            },
                                        );
                                    });
                            },
                        );
                        draw_tab_strip_separator(ui, active_tab_rect);
                    });

                let rail_frame = egui::Frame::NONE
                    .fill(chrome_panel_background_color())
                    .inner_margin(egui::Margin::symmetric(
                        RAIL_PANEL_MARGIN_X,
                        RAIL_PANEL_MARGIN_Y,
                    ));
                Panel::left("app_rail")
                    .exact_size(APP_RAIL_WIDTH)
                    .frame(rail_frame)
                    .show_separator_line(true)
                    .show_inside(ctx, |ui| {
                        Self::draw_interactive_app_rail(
                            ui,
                            slate_icons,
                            window,
                            location_dirty,
                            location,
                        )
                    });

                let toolbar_frame = egui::Frame::NONE
                    .fill(toolbar_background_color())
                    .inner_margin(egui::Margin::symmetric(
                        TOOLBAR_PANEL_MARGIN_X,
                        TOOLBAR_PANEL_MARGIN_Y,
                    ));
                Panel::top("toolbar")
                    .exact_size(TOOLBAR_HEIGHT)
                    .frame(toolbar_frame)
                    .show_separator_line(true)
                    .show_inside(ctx, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(TOOLBAR_ITEM_SPACING, 0.0);
                        ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let back_button = Gui::toolbar_navigation_button(
                                    ui,
                                    slate_icons,
                                    SlateIcon::NavBack,
                                    self.can_go_back,
                                );
                                back_button.widget_info(|| {
                                    let mut info = WidgetInfo::new(WidgetType::Button);
                                    info.label = Some("Back".into());
                                    info
                                });
                                if back_button.clicked() {
                                    *location_dirty = false;
                                    window.queue_user_interface_command(UserInterfaceCommand::Back);
                                }

                                let forward_button = Gui::toolbar_navigation_button(
                                    ui,
                                    slate_icons,
                                    SlateIcon::NavForward,
                                    self.can_go_forward,
                                );
                                forward_button.widget_info(|| {
                                    let mut info = WidgetInfo::new(WidgetType::Button);
                                    info.label = Some("Forward".into());
                                    info
                                });
                                if forward_button.clicked() {
                                    *location_dirty = false;
                                    window.queue_user_interface_command(
                                        UserInterfaceCommand::Forward,
                                    );
                                }

                                match *load_status {
                                    LoadStatus::Started | LoadStatus::HeadParsed => {
                                        let stop_button = Gui::toolbar_hover_raster_button(
                                            ui,
                                            slate_icons,
                                            SlateRaster::NavStop,
                                            SlateRaster::NavStopHover,
                                            true,
                                        );
                                        stop_button.widget_info(|| {
                                            let mut info = WidgetInfo::new(WidgetType::Button);
                                            info.label = Some("Stop".into());
                                            info
                                        });
                                        if stop_button.clicked() {
                                            warn!("Do not support stop yet.");
                                        }
                                    }
                                    LoadStatus::Complete => {
                                        let reload_button = Gui::toolbar_navigation_button(
                                            ui,
                                            slate_icons,
                                            SlateIcon::NavRefresh,
                                            true,
                                        );
                                        reload_button.widget_info(|| {
                                            let mut info = WidgetInfo::new(WidgetType::Button);
                                            info.label = Some("Reload".into());
                                            info
                                        });
                                        if reload_button.clicked() {
                                            *location_dirty = false;
                                            window.queue_user_interface_command(
                                                UserInterfaceCommand::Reload,
                                            );
                                        }
                                    }
                                }

                                ui.add_space(ADDRESS_LEADING_GAP);
                                let location_id = egui::Id::new("location_input");
                                let available_for_address = ui.available_width().max(0.0);
                                let address_width = toolbar_address_width(available_for_address);
                                let location_field = egui::Frame::NONE
                                    .fill(address_background_color())
                                    .stroke(egui::Stroke::new(1.0, address_border_color()))
                                    .corner_radius(ADDRESS_CORNER_RADIUS)
                                    .shadow(address_shadow())
                                    .inner_margin(egui::Margin::symmetric(
                                        ADDRESS_INNER_MARGIN_X,
                                        0,
                                    ))
                                    .show(ui, |ui| {
                                        ui.set_width(address_width);
                                        ui.set_min_height(ADDRESS_HEIGHT);
                                        ui.horizontal_centered(|ui| {
                                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                                            match address_security_icon_for_location(location) {
                                                AddressSecurityIcon::Slate { icon, color } => {
                                                    let page_info_icon =
                                                        slate_icons.texture(ui.ctx(), icon, color);
                                                    let (slot_rect, _) = ui.allocate_exact_size(
                                                        egui::Vec2::splat(
                                                            ADDRESS_SECURITY_ICON_SIZE,
                                                        ),
                                                        egui::Sense::hover(),
                                                    );
                                                    let icon_rect =
                                                        address_slate_security_icon_rect(slot_rect);
                                                    if ui.is_rect_visible(slot_rect) {
                                                        ui.painter().image(
                                                            page_info_icon.id,
                                                            icon_rect,
                                                            egui::Rect::from_min_max(
                                                                egui::Pos2::ZERO,
                                                                egui::pos2(1.0, 1.0),
                                                            ),
                                                            egui::Color32::WHITE,
                                                        );
                                                    }
                                                }
                                                AddressSecurityIcon::Raster(raster) => {
                                                    let page_info_icon = slate_icons
                                                        .raster_mask_texture(
                                                            ui.ctx(),
                                                            raster,
                                                            address_security_raster_color(raster),
                                                        );
                                                    ui.add(Self::icon_image(
                                                        page_info_icon,
                                                        ADDRESS_SECURITY_ICON_SIZE,
                                                    ));
                                                }
                                            }
                                            ui.add_space(ADDRESS_ICON_GAP);
                                            let bookmark_icon = slate_icons.raster_mask_texture(
                                                ui.ctx(),
                                                SlateRaster::BookmarkAdd,
                                                address_bookmark_icon_color(),
                                            );
                                            let text_width = (ui.available_width()
                                                - ADDRESS_BOOKMARK_RESERVED_WIDTH)
                                                .max(80.0);
                                            let text_response = ui.add_sized(
                                                [text_width, ADDRESS_TEXT_HEIGHT],
                                                egui::TextEdit::singleline(location)
                                                    .id(location_id)
                                                    .font(egui::FontId::proportional(
                                                        ADDRESS_INPUT_TEXT_SIZE,
                                                    ))
                                                    .frame(egui::Frame::NONE)
                                                    .hint_text(
                                                        "Search the web or enter an address",
                                                    ),
                                            );
                                            let bookmark_button = Self::address_raster_button_sized(
                                                ui,
                                                bookmark_icon,
                                                ADDRESS_BOOKMARK_ICON_SIZE,
                                            );
                                            let bookmark_clicked = bookmark_button.clicked();
                                            bookmark_button.widget_info(|| {
                                                let mut info = WidgetInfo::new(WidgetType::Button);
                                                info.label = Some("Add bookmark".into());
                                                info
                                            });
                                            bookmark_button.on_hover_text("Add bookmark");
                                            if bookmark_clicked {
                                                match Self::save_active_home_bookmark(
                                                    &state.profile_database,
                                                    window,
                                                ) {
                                                    Ok(Some(bookmarks)) => {
                                                        *home_bookmarks = bookmarks;
                                                    }
                                                    Ok(None) => {}
                                                    Err(error) => {
                                                        warn!("failed to save bookmark: {error}");
                                                    }
                                                }
                                            }
                                            text_response
                                        })
                                        .inner
                                    })
                                    .inner;

                                if active_webview_is_blank
                                    && !*location_dirty
                                    && !location_field.has_focus()
                                {
                                    location_field.request_focus();
                                }
                                if location_field.changed() {
                                    *location_dirty = true;
                                }
                                // Handle adddress bar shortcut.
                                if ui.input(|i| {
                                    if cfg!(target_os = "macos") {
                                        i.clone().consume_key(Modifiers::COMMAND, Key::L)
                                    } else {
                                        i.clone().consume_key(Modifiers::COMMAND, Key::L)
                                            || i.clone().consume_key(Modifiers::ALT, Key::D)
                                    }
                                }) {
                                    // The focus request immediately makes gained_focus return true.
                                    location_field.request_focus();
                                }
                                // Select address bar text when it's focused (click or shortcut).
                                if location_field.gained_focus()
                                    && let Some(mut state) =
                                        TextEditState::load(ui.ctx(), location_id)
                                {
                                    // Select the whole input.
                                    state.cursor.set_char_range(Some(CCursorRange::two(
                                        CCursor::new(0),
                                        CCursor::new(location.len()),
                                    )));
                                    state.store(ui.ctx(), location_id);
                                }
                                // Navigate to address when enter is pressed in the address bar.
                                if location_field.lost_focus()
                                    && ui.input(|i| i.clone().key_pressed(Key::Enter))
                                {
                                    window.queue_user_interface_command(UserInterfaceCommand::Go(
                                        location.clone(),
                                    ));
                                }

                                ui.add_space(ADDRESS_TRAILING_GAP);
                                let privacy_icon = slate_icons.texture(
                                    ui.ctx(),
                                    SlateIcon::TopShield,
                                    slate_theme::AMBER,
                                );
                                let privacy_button = Gui::toolbar_icon_button_sized(
                                    ui,
                                    privacy_icon,
                                    TOOLBAR_PRIVACY_ICON_SIZE,
                                );
                                privacy_button.widget_info(|| {
                                    let mut info = WidgetInfo::new(WidgetType::Button);
                                    info.label = Some("Privacy controls".into());
                                    info
                                });
                                privacy_button.on_hover_text("Privacy controls");

                                let toolbar_item_spacing = ui.spacing().item_spacing.x;
                                ui.spacing_mut().item_spacing.x = TOOLBAR_SEPARATOR_LEADING_GAP;
                                Self::vertical_separator(ui, TOOLBAR_SEPARATOR_HEIGHT);

                                ui.spacing_mut().item_spacing.x = TOOLBAR_SEPARATOR_TRAILING_GAP;
                                let menu_button = Gui::toolbar_menu_button(
                                    ui,
                                    state.experimental_preferences_enabled(),
                                )
                                .on_hover_text("Menu");
                                ui.spacing_mut().item_spacing.x = toolbar_item_spacing;
                                menu_button.widget_info(|| {
                                    let mut info = WidgetInfo::new(WidgetType::Button);
                                    info.label = Some("Menu".into());
                                    info
                                });
                                Self::draw_toolbar_menu(
                                    &menu_button,
                                    state,
                                    window,
                                    location_dirty,
                                    toolbar_menu_popup_id,
                                );
                            },
                        );
                    });

                let footer_frame = egui::Frame::NONE
                    .fill(chrome_panel_background_color())
                    .inner_margin(footer_panel_margin());
                let footer_response = Panel::bottom("footer")
                    .exact_size(FOOTER_HEIGHT)
                    .frame(footer_frame)
                    .show_separator_line(false)
                    .show_inside(ctx, |ui| {
                        Self::draw_footer(ui, *load_status, broadweb_status, location)
                    });
                Self::draw_footer_top_separator(ctx, footer_response.response.rect);
            } else {
                *toolbar_height = Length::default();
                *webview_origin = Point2D::zero();
                *webview_size = Size2D::zero();
            }

            let scale =
                Scale::<_, DeviceIndependentPixel, DevicePixel>::new(ctx.pixels_per_point());

            headed_window.for_each_active_dialog(window, |dialog| dialog.update(ctx));

            // If the top parts of the GUI changed size, then update the size of the WebView and also
            // the size of its RenderingContext.
            let available_rect = ctx.available_rect_before_wrap();
            *toolbar_height = Length::new(available_rect.min.y);
            *webview_origin = Point2D::new(available_rect.min.x, available_rect.min.y);
            *webview_size = Size2D::new(available_rect.width(), available_rect.height());

            // Build a graft node for each WebView.
            for (webview_id, webview) in window.webviews() {
                if let Some(tree_id) = webview.accesskit_tree_id() {
                    let id = egui::Id::new(webview_id);
                    ctx.accesskit_node_builder(id, |node| {
                        node.set_tree_id(tree_id);
                    });
                }
            }
            let size = Size2D::new(available_rect.width(), available_rect.height()) * scale;
            if let Some(webview) = window.active_webview()
                && size != webview.size()
            {
                // `rect` is sized to just the WebView viewport, which is required by
                // `OffscreenRenderingContext` See:
                // <https://github.com/servo/servo/issues/38369#issuecomment-3138378527>
                webview.resize(PhysicalSize::new(size.width as u32, size.height as u32))
            }

            if let Some(active_cards) = match active_native_chrome_page {
                Some(NativeChromePage::Home) => Some(home_bookmarks.as_slice()),
                Some(NativeChromePage::Web) => Some(web_history_cards.as_slice()),
                None => None,
            } {
                Self::draw_home_view(
                    ctx,
                    available_rect,
                    slate_icons,
                    home_favicon_textures,
                    home_search,
                    active_cards,
                    window,
                );
            }

            if let Some(status_text) = &self.status_text {
                Self::draw_status_text(ctx, available_rect, status_text);
            }

            if *load_status != LoadStatus::Complete
                || matches!(
                    broadweb_status.kind,
                    BroadwebStatusKind::Fetching | BroadwebStatusKind::SwitchingGateway
                )
            {
                ctx.request_repaint_after(Duration::from_millis(100));
            }

            if active_native_chrome_page.is_none() {
                window.repaint_webviews();

                if let Some(render_to_parent) = rendering_context.render_to_parent_callback() {
                    ctx.layer_painter(LayerId::background()).add(PaintCallback {
                        rect: available_rect,
                        callback: Arc::new(CallbackFn::new(move |info, painter| {
                            let clip = info.viewport_in_pixels();
                            let rect_in_parent = Rect::new(
                                Point2D::new(clip.left_px, clip.from_bottom_px),
                                Size2D::new(clip.width_px, clip.height_px),
                            );
                            render_to_parent(painter.gl(), rect_in_parent)
                        })),
                    });
                }
            }
        });

        // If any egui widget requested a repaint, also request a repaint for our
        // containing window. This allows egui widget to animate on their own.
        if self.context.egui_ctx.has_requested_repaint() {
            window.set_needs_repaint();
        }

        let adapter = self
            .context
            .egui_winit
            .accesskit
            .as_mut()
            .expect("guaranteed by Gui::new()");
        for tree_update in self.pending_accesskit_updates.drain(..) {
            adapter.update_if_active(|| tree_update);
        }
    }

    /// Paint the GUI, as of the last update.
    pub(crate) fn paint(&mut self, window: &Window) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        self.rendering_context
            .parent_context()
            .prepare_for_rendering();
        self.context.paint(window);
        self.rendering_context.parent_context().present();
    }

    /// Updates the location field from the given [`RunningAppState`], unless the user has started
    /// editing it without clicking Go, returning true iff it has changed (needing an egui update).
    fn update_location_in_toolbar(&mut self, window: &ServoShellWindow) -> bool {
        // User edited without clicking Go?
        if self.location_dirty {
            return false;
        }

        let current_url_string = window
            .active_webview()
            .and_then(|webview| Some(location_for_toolbar(&webview.url()?)));
        match current_url_string {
            Some(location) if location != self.location => {
                self.location = location;
                true
            }
            _ => false,
        }
    }

    fn update_load_status(&mut self, window: &ServoShellWindow) -> bool {
        let state_status = window
            .active_webview()
            .map(|webview| webview.load_status())
            .unwrap_or(LoadStatus::Complete);
        let old_status = std::mem::replace(&mut self.load_status, state_status);
        let status_changed = old_status != self.load_status;

        // When the load status changes, we want the new changes to the URL to start
        // being reflected in the location bar.
        if status_changed {
            self.location_dirty = false;
        }

        status_changed
    }

    fn save_active_home_bookmark(
        database: &SlateProfileDatabase,
        window: &ServoShellWindow,
    ) -> Result<Option<Vec<HomeBookmarkCard>>, StorageError> {
        let Some(webview) = window.active_webview() else {
            return Ok(None);
        };
        let Some(url) = webview.url().map(|url| url.to_string()) else {
            return Ok(None);
        };
        if !is_home_bookmarkable_url(&url) {
            return Ok(None);
        }

        let records = home_bookmark_records_from_database(database)?;
        let slot = home_bookmark_slot_for_url(&records, &url);
        let replaced_url = records.get(slot).map(|bookmark| bookmark.url.as_str());
        database.set_bookmark_slot(
            &BookmarkUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                url: url.clone(),
                title: Some(home_bookmark_title(webview.page_title(), &url)),
                folder: None,
                position: slot as i64,
                favicon_key: Some(home_bookmark_favicon_key(&url)),
            },
            replaced_url,
        )?;

        home_bookmark_cards_from_database(database).map(Some)
    }

    fn update_home_bookmark_favicons(
        ctx: &egui::Context,
        database: &SlateProfileDatabase,
        bookmarks: &[HomeBookmarkCard],
        textures: &mut HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
        pending: &mut HashSet<String>,
        failed: &mut HashSet<String>,
        receiver: &Receiver<HomeFaviconFetchResult>,
        sender: &Sender<HomeFaviconFetchResult>,
    ) -> bool {
        let mut changed =
            Self::drain_home_favicon_results(ctx, database, textures, pending, failed, receiver);

        for bookmark in bookmarks.iter().take(HOME_BOOKMARK_SLOT_COUNT) {
            let Some(key) = bookmark.favicon_key.as_deref() else {
                continue;
            };
            let Some(favicon_url) = bookmark.favicon_url.as_deref() else {
                continue;
            };
            if textures.contains_key(key) || pending.contains(key) || failed.contains(key) {
                continue;
            }

            match database.get_blob(DEFAULT_PROFILE_ID, key) {
                Ok(Some(blob)) => {
                    if let Some(texture) = load_home_favicon_texture(ctx, key, &blob.data) {
                        textures.insert(key.to_string(), texture);
                        changed = true;
                        continue;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    warn!("failed to load cached bookmark favicon {key}: {error}");
                }
            }

            pending.insert(key.to_string());
            spawn_home_favicon_fetch(sender.clone(), key.to_string(), favicon_url.to_string());
        }

        if !pending.is_empty() {
            ctx.request_repaint_after(HOME_FAVICON_FETCH_REPAINT_INTERVAL);
        }

        changed
    }

    fn drain_home_favicon_results(
        ctx: &egui::Context,
        database: &SlateProfileDatabase,
        textures: &mut HashMap<String, (egui::TextureHandle, egui::load::SizedTexture)>,
        pending: &mut HashSet<String>,
        failed: &mut HashSet<String>,
        receiver: &Receiver<HomeFaviconFetchResult>,
    ) -> bool {
        let mut changed = false;
        loop {
            match receiver.try_recv() {
                Ok(result) => {
                    pending.remove(&result.key);
                    match result.result {
                        Ok(favicon) => {
                            if let Some(texture) =
                                load_home_favicon_texture(ctx, &result.key, &favicon.bytes)
                            {
                                if let Err(error) = database.set_blob(
                                    DEFAULT_PROFILE_ID,
                                    &result.key,
                                    favicon.media_type.as_deref(),
                                    &favicon.bytes,
                                ) {
                                    warn!(
                                        "failed to cache bookmark favicon {}: {error}",
                                        result.key
                                    );
                                }
                                failed.remove(&result.key);
                                textures.insert(result.key, texture);
                                changed = true;
                            } else {
                                warn!("failed to decode bookmark favicon {}", result.key);
                                failed.insert(result.key);
                            }
                        }
                        Err(error) => {
                            warn!("failed to fetch bookmark favicon {}: {error}", result.key);
                            failed.insert(result.key);
                        }
                    }
                }
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => return changed,
            }
        }
    }

    fn update_home_bookmarks(&mut self, database: &SlateProfileDatabase) -> bool {
        if self.home_bookmarks_loaded {
            return false;
        }
        self.home_bookmarks_loaded = true;

        let Ok(bookmarks) = home_bookmark_cards_from_database(database).inspect_err(|error| {
            warn!("failed to load home bookmarks: {error}");
        }) else {
            return false;
        };

        if bookmarks == self.home_bookmarks {
            return false;
        }

        self.home_bookmarks = bookmarks;
        true
    }

    fn update_web_history(&mut self, database: &SlateProfileDatabase) -> bool {
        let Ok(history) = web_history_cards_from_database(database).inspect_err(|error| {
            warn!("failed to load web history: {error}");
        }) else {
            return false;
        };

        if history == self.web_history_cards {
            return false;
        }

        self.web_history_cards = history;
        true
    }

    fn update_status_text(&mut self, window: &ServoShellWindow) -> bool {
        let state_status = window
            .active_webview()
            .and_then(|webview| webview.status_text());
        let old_status = std::mem::replace(&mut self.status_text, state_status);
        old_status != self.status_text
    }

    fn update_broadweb_status(&mut self) -> bool {
        let state_status = default_session_status_snapshot();
        let old_status = std::mem::replace(&mut self.broadweb_status, state_status);
        old_status != self.broadweb_status
    }

    fn update_chrome_element_zoom(&mut self, window: &ServoShellWindow) -> bool {
        let settings_url_zoom = window
            .active_webview()
            .and_then(|webview| webview.url())
            .and_then(|url| {
                let zoom = chrome_element_zoom_from_settings_url(&url)?;
                Some((url.to_string(), zoom))
            });

        match settings_url_zoom {
            Some((url, url_zoom))
                if self.last_chrome_element_zoom_url.as_deref() != Some(url.as_str()) =>
            {
                set_current_chrome_element_zoom_setting(url_zoom);
                self.last_chrome_element_zoom_url = Some(url);
            }
            Some((url, _)) => {
                self.last_chrome_element_zoom_url = Some(url);
            }
            None => {
                self.last_chrome_element_zoom_url = None;
            }
        }

        let state_zoom = current_chrome_element_zoom_setting();
        let old_zoom = std::mem::replace(&mut self.chrome_element_zoom, state_zoom);
        (old_zoom - self.chrome_element_zoom).abs() > 0.001
    }

    fn update_can_go_back_and_forward(&mut self, window: &ServoShellWindow) -> bool {
        let (can_go_back, can_go_forward) = window
            .active_webview()
            .map(|webview| (webview.can_go_back(), webview.can_go_forward()))
            .unwrap_or((false, false));
        let old_can_go_back = std::mem::replace(&mut self.can_go_back, can_go_back);
        let old_can_go_forward = std::mem::replace(&mut self.can_go_forward, can_go_forward);
        old_can_go_back != self.can_go_back || old_can_go_forward != self.can_go_forward
    }

    /// Updates all fields taken from the given [`ServoShellWindow`], such as the location field.
    /// Returns true iff the egui needs an update.
    pub(crate) fn update_webview_data(&mut self, window: &ServoShellWindow) -> bool {
        // Note: We must use the "bitwise OR" (|) operator here instead of "logical OR" (||)
        //       because logical OR would short-circuit if any of the functions return true.
        //       We want to ensure that all functions are called. The "bitwise OR" operator
        //       does not short-circuit.
        self.update_load_status(window)
            | self.update_location_in_toolbar(window)
            | self.update_status_text(window)
            | self.update_broadweb_status()
            | self.update_chrome_element_zoom(window)
            | self.update_can_go_back_and_forward(window)
    }

    /// Returns true if a redraw is required after handling the provided event.
    pub(crate) fn handle_accesskit_event(
        &mut self,
        event: &egui_winit::accesskit_winit::WindowEvent,
    ) -> bool {
        match event {
            egui_winit::accesskit_winit::WindowEvent::InitialTreeRequested => {
                self.context.egui_ctx.enable_accesskit();
                true
            }
            egui_winit::accesskit_winit::WindowEvent::ActionRequested(req) => {
                self.context
                    .egui_winit
                    .on_accesskit_action_request(req.clone());
                true
            }
            egui_winit::accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                self.context.egui_ctx.disable_accesskit();
                false
            }
        }
    }

    fn effective_egui_zoom_factor(&self) -> f32 {
        self.platform_zoom_factor.get() * chrome_element_zoom_factor(self.chrome_element_zoom)
    }

    pub(crate) fn set_zoom_factor(&self, factor: f32) {
        self.platform_zoom_factor.set(factor);
        self.context
            .egui_ctx
            .set_zoom_factor(self.effective_egui_zoom_factor());
    }

    pub(crate) fn notify_accessibility_tree_update(&mut self, tree_update: accesskit::TreeUpdate) {
        self.pending_accesskit_updates.push(tree_update);
    }
}

#[cfg(test)]
mod tests {
    use euclid::{Point2D, Size2D};
    use servo::{DeviceIndependentPixel, LoadStatus};
    use slate_broadwebd::{BroadwebStatusKind, BroadwebStatusSnapshot};
    use slate_storage::{
        BookmarkRecord, DEFAULT_HOME_BOOKMARKS, DEFAULT_PROFILE_ID, HistoryVisitRecord,
    };
    use url::Url;

    use super::{
        ACTIVE_TAB_BOTTOM_JOIN_HEIGHT, ACTIVE_TAB_BOTTOM_JOIN_INSET_X,
        ACTIVE_TAB_FILE_CORNER_STEPS, ADDRESS_HEIGHT, ADDRESS_INPUT_TEXT_SIZE, APP_RAIL_WIDTH,
        APP_TITLE_HEIGHT, APP_TITLE_LEFT_PADDING, APP_TITLE_TEXT_SIZE, APP_TITLE_WIDTH,
        CHROME_ELEMENT_ZOOM, CHROME_ELEMENT_ZOOM_MAX, CHROME_ELEMENT_ZOOM_MIN,
        CONCEPT_SCREENSHOT_HEIGHT, CONCEPT_SCREENSHOT_WIDTH, FOOTER_HEIGHT, FOOTER_LEFT_PADDING,
        FOOTER_LOAD_STATUS_DOT_LABEL_GAP, FOOTER_LOAD_STATUS_DOT_SIZE, FOOTER_LOAD_STATUS_HEIGHT,
        FOOTER_PANEL_MARGIN_BOTTOM, FOOTER_PANEL_MARGIN_TOP, FOOTER_PANEL_MARGIN_X,
        FOOTER_RIGHT_PADDING, FOOTER_TEXT_SIZE, HOME_BOTTOM_MIN_GAP, HOME_CONTENT_OPTICAL_OFFSET_X,
        HOME_FAVICON_MAX_BYTES, HOME_HERO_MOTTO_GAP, HOME_HERO_OPTICAL_OFFSET_X, HOME_HERO_SIZE,
        HOME_HERO_TO_SEARCH_GAP, HOME_METRIC_BADGE_CORNER_RADIUS,
        HOME_METRIC_BADGE_EXTRA_DIGIT_FACTOR, HOME_METRIC_BADGE_LABEL_GAP,
        HOME_METRIC_BADGE_MARGIN_X, HOME_METRIC_BADGE_MARGIN_Y,
        HOME_METRIC_BADGE_PRIMARY_DIGIT_FACTOR, HOME_METRIC_BADGE_TEXT_SIZE, HOME_METRIC_CARD_GAP,
        HOME_METRIC_CARD_HEIGHT, HOME_METRIC_CARD_INNER_MARGIN_X, HOME_METRIC_CARD_INNER_MARGIN_Y,
        HOME_METRIC_CARD_MAX_WIDTH, HOME_METRIC_DETAIL_GAP, HOME_METRIC_DETAIL_TEXT_SIZE,
        HOME_METRIC_GRID_EXTRA_HEIGHT, HOME_METRIC_ICON_LABEL_GAP, HOME_METRIC_ICON_SIZE,
        HOME_METRIC_LABEL_TEXT_SIZE, HOME_MOTTO_HEIGHT, HOME_MOTTO_TEXT_SIZE, HOME_MOTTO_WIDTH,
        HOME_PANEL_SHADOW_ALPHA, HOME_PANEL_SHADOW_BLUR, HOME_PANEL_SHADOW_OFFSET,
        HOME_PANEL_SHADOW_SPREAD, HOME_SEARCH_FRAME_EXTRA_HEIGHT, HOME_SEARCH_ICON_OFFSET_Y,
        HOME_SEARCH_ICON_SIZE, HOME_SEARCH_INPUT_TEXT_SIZE, HOME_SEARCH_TO_METRICS_GAP,
        HOME_TOP_SPACE_FACTOR, HOME_TOP_SPACE_MAX, HOME_TOP_SPACE_MIN, NEW_TAB_BUTTON_RADIUS,
        NEW_TAB_BUTTON_SIZE, NEW_TAB_ICON_SIZE, NEW_TAB_ICON_STROKE, NEW_TAB_LEFT_GAP,
        NEW_TAB_SLOT_HEIGHT, STATUS_BUBBLE_CORNER_RADIUS, STATUS_BUBBLE_HEIGHT,
        STATUS_BUBBLE_HORIZONTAL_PADDING, STATUS_BUBBLE_MARGIN_X, STATUS_BUBBLE_MARGIN_Y,
        STATUS_BUBBLE_MAX_WIDTH, STATUS_BUBBLE_SHADOW_ALPHA, STATUS_TEXT_SIZE,
        TAB_CLOSE_BUTTON_RADIUS, TAB_CLOSE_ICON_SIZE, TAB_CONCEPT_WINDOW_WIDTH, TAB_CONTENT_ALIGN,
        TAB_CONTENT_HEIGHT, TAB_CORNER_RADIUS, TAB_HEIGHT, TAB_ICON_SIZE, TAB_ICON_TITLE_GAP,
        TAB_INNER_MARGIN_X, TAB_INNER_MARGIN_Y, TAB_MIN_WIDTH, TAB_OPENING_PREFERRED_WIDTH,
        TAB_OPENING_WINDOW_WIDTH, TAB_STRIP_CONTENT_ALIGN, TAB_STRIP_HEIGHT, TAB_TITLE_CLOSE_GAP,
        TAB_TITLE_MIN_WIDTH, TAB_TITLE_TEXT_SIZE, TAB_WIDTH, TOOLBAR_BUTTON_RADIUS,
        TOOLBAR_BUTTON_SIZE, TOOLBAR_HEIGHT, TOOLBAR_ICON_SIZE, TOOLBAR_ITEM_SPACING,
        TOOLBAR_MENU_ICON_GAP, TOOLBAR_MENU_ICON_OFFSET_X, TOOLBAR_MENU_ICON_STROKE,
        TOOLBAR_MENU_ICON_WIDTH, TOOLBAR_NAV_BACK_ICON_OFFSET_X, TOOLBAR_NAV_FORWARD_ICON_OFFSET_X,
        TOOLBAR_NAV_ICON_SIZE, TOOLBAR_NAV_REFRESH_ICON_OFFSET_X, TOOLBAR_PANEL_MARGIN_X,
        TOOLBAR_PANEL_MARGIN_Y, TOOLBAR_PRIVACY_ICON_SIZE, TOOLBAR_SEPARATOR_HEIGHT,
        TOOLBAR_SEPARATOR_LEADING_GAP, TOOLBAR_SEPARATOR_TRAILING_GAP,
        egui_chrome_captures_mouse_position, egui_chrome_owns_position,
    };
    use super::{
        ADDRESS_BOOKMARK_BUTTON_RADIUS, ADDRESS_BOOKMARK_BUTTON_SIZE, ADDRESS_BOOKMARK_ICON_SIZE,
        ADDRESS_BOOKMARK_RESERVED_WIDTH, ADDRESS_CORNER_RADIUS, ADDRESS_ICON_GAP,
        ADDRESS_INNER_MARGIN_X, ADDRESS_LEADING_GAP, ADDRESS_MIN_WIDTH, ADDRESS_SECURITY_ICON_SIZE,
        ADDRESS_SHADOW_ALPHA, ADDRESS_SHADOW_BLUR, ADDRESS_SHADOW_OFFSET, ADDRESS_SHADOW_SPREAD,
        ADDRESS_SLATE_SECURITY_ICON_OFFSET_X, ADDRESS_SLATE_SECURITY_ICON_SIZE,
        ADDRESS_TEXT_HEIGHT, ADDRESS_TRAILING_CONTROLS_WIDTH, ADDRESS_TRAILING_GAP,
        AddressSecurityIcon, Gui, HOME_METRIC_CARD_MIN_WIDTH, HomeContentLayout, SlateIconCache,
        active_tab_background_color, active_tab_content_divider_points, active_tab_outline_color,
        active_tab_separator_join, address_background_color, address_bookmark_icon_color,
        address_bookmark_icon_rect, address_border_color, address_outer_width,
        address_passive_icon_color, address_security_icon_for_location,
        address_security_raster_color, address_slate_security_icon_rect,
        address_slate_security_visible_rect, app_title_background_color, app_title_text_color,
        chrome_element_zoom_factor, chrome_element_zoom_from_settings_url,
        chrome_panel_background_color, chrome_vertical_separator_color, clamp_chrome_element_zoom,
        concept_chrome_geometry, concept_footer_controls_geometry,
        concept_screenshot_home_view_size, concept_toolbar_controls_geometry,
        default_home_bookmark_cards, default_opening_home_view_height,
        default_opening_home_view_size, footer_load_status_dot_radius,
        footer_load_status_indicator_color, footer_load_status_indicator_color_at,
        footer_load_status_is_in_progress, footer_load_status_label_max_chars,
        footer_load_status_pulse_target_color, footer_load_status_width, footer_panel_margin,
        footer_status_text_color, footer_top_separator_color, home_bookmark_favicon_key,
        home_bookmark_favicon_url, home_bookmark_slot_for_url, home_bookmark_title,
        home_content_left_space, home_content_stack_height, home_favicon_color_image,
        home_hero_icon_visible_rect, home_hero_left_space, home_metric_badge_width,
        home_metric_card_background_color, home_metric_card_content_height,
        home_metric_card_content_width, home_metric_detail_color, home_metrics_layout,
        home_metrics_rendered_height, home_metrics_row_width, home_search_background_color,
        home_search_border_color, home_search_icon_color, home_search_icon_rect,
        home_search_icon_visible_rect, home_search_rendered_height, home_search_width,
        home_top_space, home_view_background_color, inactive_tab_background_color,
        inactive_tab_hover_background_color, inactive_tab_outline_color,
        inactive_tab_outline_points, is_home_bookmarkable_url, location_for_toolbar,
        location_is_downloads, location_is_home, location_is_web, new_tab_icon_color,
        rail_button_fill, rail_icon_color, rail_selected_button_fill, slate_theme,
        status_bubble_label, status_bubble_width, tab_close_button_rect, tab_close_icon_color,
        tab_close_raster, tab_content_width, tab_corner_radius, tab_icon_color, tab_icon_slot_rect,
        tab_strip_background_color, tab_strip_separator_color, tab_title_color, tab_title_left,
        tab_title_width, tab_width_for_strip, toolbar_address_width, toolbar_background_color,
        toolbar_menu_icon_center, toolbar_menu_icon_color, toolbar_menu_icon_rect,
        toolbar_navigation_icon_color, toolbar_navigation_icon_offset_x,
        toolbar_navigation_icon_rect, toolbar_navigation_raster, web_history_cards_from_records,
    };
    use super::{
        HOME_SEARCH_CORNER_RADIUS, HOME_SEARCH_HEIGHT, HOME_SEARCH_HORIZONTAL_PADDING,
        HOME_SEARCH_ICON_GAP, HOME_SEARCH_INNER_MARGIN_X, HOME_SEARCH_MAX_WIDTH,
        HOME_SEARCH_MIN_WIDTH, HOME_SEARCH_TEXT_HEIGHT, HOME_SEARCH_WIDTH_FACTOR,
    };
    use super::{
        RAIL_BUTTON_RADIUS, RAIL_BUTTON_SIZE, RAIL_ICON_SIZE, RAIL_ITEM_GAP, RAIL_PANEL_MARGIN_X,
        RAIL_PANEL_MARGIN_Y, RAIL_TOP_SPACE, TAB_CLOSE_BUTTON_SIZE,
    };

    const LAYOUT_EPSILON: f32 = 1.0;

    fn chrome_webview_origin() -> Point2D<f32, DeviceIndependentPixel> {
        Point2D::new(APP_RAIL_WIDTH, TAB_STRIP_HEIGHT + TOOLBAR_HEIGHT)
    }

    fn bookmark_record(url: &str, position: i64) -> BookmarkRecord {
        BookmarkRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            url: url.to_string(),
            title: Some(url.to_string()),
            folder: None,
            position,
            favicon_key: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn history_record(url: &str, title: Option<&str>, last_visited_at: i64) -> HistoryVisitRecord {
        HistoryVisitRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            url: url.to_string(),
            title: title.map(ToOwned::to_owned),
            first_visited_at: 0,
            last_visited_at,
            visit_count: 1,
        }
    }

    fn rect_has_area(rect: egui::Rect) -> bool {
        rect.width() > 0.0 && rect.height() > 0.0
    }

    fn rect_is_inside(outer: egui::Rect, inner: egui::Rect) -> bool {
        inner.min.x >= outer.min.x - LAYOUT_EPSILON
            && inner.min.y >= outer.min.y - LAYOUT_EPSILON
            && inner.max.x <= outer.max.x + LAYOUT_EPSILON
            && inner.max.y <= outer.max.y + LAYOUT_EPSILON
    }

    fn assert_rect_close(actual: egui::Rect, expected: egui::Rect) {
        assert!(
            (actual.min.x - expected.min.x).abs() < LAYOUT_EPSILON
                && (actual.min.y - expected.min.y).abs() < LAYOUT_EPSILON
                && (actual.max.x - expected.max.x).abs() < LAYOUT_EPSILON
                && (actual.max.y - expected.max.y).abs() < LAYOUT_EPSILON,
            "expected {actual:?} to match {expected:?}"
        );
    }

    fn points_are_close(actual: egui::Pos2, expected: egui::Pos2) -> bool {
        (actual.x - expected.x).abs() < 0.01 && (actual.y - expected.y).abs() < 0.01
    }

    fn render_home_content_layout(viewport_size: egui::Vec2) -> HomeContentLayout {
        let ctx = egui::Context::default();
        slate_theme::apply(&ctx);

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size);
        let input = egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let mut slate_icons = SlateIconCache::default();
        let mut home_search = String::new();
        let mut layout = None;

        let _ = ctx.run_ui(input, |ui| {
            let response = Gui::draw_home_content(
                ui,
                screen_rect,
                &mut slate_icons,
                &std::collections::HashMap::new(),
                &mut home_search,
                &default_home_bookmark_cards(),
            );
            layout = Some(response.layout);
        });

        layout.expect("home content should be rendered")
    }

    #[test]
    fn normal_webview_area_forwards_inside_points_to_servo() {
        let origin = chrome_webview_origin();
        let size = Size2D::<f32, DeviceIndependentPixel>::new(900.0, 500.0);
        let inside_webview = Point2D::new(origin.x + 36.0, origin.y + 36.0);

        assert!(!egui_chrome_owns_position(
            origin,
            size,
            false,
            inside_webview,
        ));
    }

    #[test]
    fn native_home_area_keeps_inside_points_in_egui() {
        let origin = chrome_webview_origin();
        let size = Size2D::<f32, DeviceIndependentPixel>::new(900.0, 500.0);
        let inside_webview = Point2D::new(origin.x + 36.0, origin.y + 36.0);

        assert!(egui_chrome_owns_position(
            origin,
            size,
            true,
            inside_webview,
        ));
    }

    #[test]
    fn chrome_keeps_points_outside_the_webview() {
        let origin = chrome_webview_origin();
        let size = Size2D::<f32, DeviceIndependentPixel>::new(900.0, 500.0);

        assert!(egui_chrome_owns_position(
            origin,
            size,
            false,
            Point2D::new(30.0, origin.y + 36.0),
        ));
    }

    #[test]
    fn open_chrome_popup_captures_mouse_inside_webview_area() {
        let origin = chrome_webview_origin();
        let size = Size2D::<f32, DeviceIndependentPixel>::new(900.0, 500.0);
        let inside_webview = Point2D::new(origin.x + 36.0, origin.y + 36.0);

        assert!(!egui_chrome_captures_mouse_position(
            origin,
            size,
            false,
            false,
            inside_webview,
        ));
        assert!(egui_chrome_captures_mouse_position(
            origin,
            size,
            false,
            true,
            inside_webview,
        ));
    }

    #[test]
    fn rail_downloads_selection_matches_downloads_internal_page() {
        assert!(location_is_downloads("slate://downloads"));
        assert!(location_is_downloads("slate:downloads"));
        assert!(!location_is_downloads("slate://home"));
        assert!(!location_is_downloads("https://example.com"));
    }

    #[test]
    fn rail_page_selection_matches_home_and_web_internal_pages() {
        assert!(location_is_home("slate://home"));
        assert!(location_is_home("slate:home"));
        assert!(!location_is_home("slate://web"));

        assert!(location_is_web("slate://web"));
        assert!(location_is_web("slate:web"));
        assert!(!location_is_web("slate://home"));
    }

    #[test]
    fn blank_internal_url_displays_empty_location() {
        assert_eq!(
            location_for_toolbar(&Url::parse("slate://blank").unwrap()),
            ""
        );
        assert_eq!(
            location_for_toolbar(&Url::parse("slate://home").unwrap()),
            "slate://home"
        );
    }

    #[test]
    fn web_history_cards_use_recent_external_history() {
        let cards = web_history_cards_from_records(vec![
            history_record("slate://settings", Some("Settings"), 5),
            history_record("https://example.com/", Some("Example"), 4),
            history_record("ipfs://bafybeigdyrzt/readme.txt", None, 3),
            history_record("file:///tmp/local.html", Some("Local"), 2),
            history_record("https://servo.org/", Some("Servo"), 1),
        ]);

        assert_eq!(cards.len(), 4);
        assert_eq!(cards[0].label, "Example");
        assert_eq!(cards[0].url.as_deref(), Some("https://example.com/"));
        assert_eq!(cards[1].label, "ipfs://bafybeigdyrzt");
        assert_eq!(
            cards[1].url.as_deref(),
            Some("ipfs://bafybeigdyrzt/readme.txt")
        );
        assert_eq!(cards[2].label, "Servo");
        assert_eq!(cards[2].url.as_deref(), Some("https://servo.org/"));
        assert_eq!(cards[3].label, "No history yet");
        assert!(cards[3].url.is_none());
    }

    #[test]
    fn static_chrome_dimensions_match_concept_offsets() {
        assert_eq!(CONCEPT_SCREENSHOT_WIDTH, 1672.0);
        assert_eq!(CONCEPT_SCREENSHOT_HEIGHT, 941.0);
        assert_eq!(CHROME_ELEMENT_ZOOM, 0.9);
        assert!((APP_RAIL_WIDTH - 93.6).abs() < 0.001);
        assert!((RAIL_ICON_SIZE - 36.0).abs() < 0.001);
        assert!((RAIL_BUTTON_SIZE - 72.0).abs() < 0.001);
        assert_eq!(RAIL_BUTTON_RADIUS, 8);
        assert_eq!(RAIL_PANEL_MARGIN_X, 12);
        assert_eq!(RAIL_PANEL_MARGIN_Y, 0);
        assert!((RAIL_TOP_SPACE - 19.8).abs() < 0.001);
        assert!((RAIL_ITEM_GAP - 10.8).abs() < 0.001);
        assert!((TAB_STRIP_HEIGHT + TOOLBAR_HEIGHT - 153.6).abs() < 0.001);
        assert_eq!(tab_strip_background_color(), slate_theme::TITLE_SURFACE);
        assert_eq!(tab_strip_separator_color(), slate_theme::FIELD_BORDER);
        assert_eq!(chrome_panel_background_color(), slate_theme::CHROME_BG);
        assert_eq!(toolbar_background_color(), slate_theme::FIELD_SURFACE);
        assert_eq!(TAB_STRIP_CONTENT_ALIGN, egui::Align::Max);
        assert_eq!(TAB_CONTENT_ALIGN, egui::Align::Center);
        assert_eq!(ACTIVE_TAB_BOTTOM_JOIN_HEIGHT, 4.0);
        assert_eq!(ACTIVE_TAB_BOTTOM_JOIN_INSET_X, 0.0);
        assert_eq!(ACTIVE_TAB_FILE_CORNER_STEPS, 5);
        assert_eq!(FOOTER_HEIGHT, 44.0);
        assert_eq!(FOOTER_PANEL_MARGIN_X, 0);
        assert_eq!(FOOTER_PANEL_MARGIN_TOP, 4);
        assert_eq!(FOOTER_PANEL_MARGIN_BOTTOM, 4);
        let footer_margin = footer_panel_margin();
        assert_eq!(footer_margin.left, 0);
        assert_eq!(footer_margin.right, 0);
        assert_eq!(footer_margin.top, 4);
        assert_eq!(footer_margin.bottom, 4);
        assert_eq!(FOOTER_LEFT_PADDING, 16.0);
        assert_eq!(FOOTER_RIGHT_PADDING, 12.0);
        assert_eq!(FOOTER_TEXT_SIZE, 13.0);
        assert_eq!(
            chrome_vertical_separator_color(),
            egui::Color32::from_rgb(225, 225, 225)
        );
        assert_eq!(
            footer_top_separator_color(),
            egui::Color32::from_rgb(241, 240, 239)
        );
        assert_eq!(FOOTER_LOAD_STATUS_DOT_SIZE, 10.0);
        assert_eq!(
            footer_load_status_indicator_color(LoadStatus::Complete),
            egui::Color32::from_rgb(11, 126, 121)
        );
        assert_eq!(
            footer_load_status_indicator_color(LoadStatus::Started),
            egui::Color32::from_rgb(202, 132, 34)
        );
        assert_eq!(
            footer_load_status_indicator_color(LoadStatus::HeadParsed),
            slate_theme::BLUE
        );
        assert_eq!(
            footer_load_status_pulse_target_color(),
            egui::Color32::from_rgb(172, 172, 168)
        );
        assert_eq!(footer_load_status_dot_radius(), 5.0);
        assert_eq!(FOOTER_LOAD_STATUS_DOT_LABEL_GAP, 8.0);
        assert_eq!(FOOTER_LOAD_STATUS_HEIGHT, 28.0);
        assert_eq!(footer_load_status_width(320.0), 320.0);
        assert_eq!(footer_load_status_label_max_chars(320.0), 37);
        assert_eq!(
            footer_status_text_color(),
            egui::Color32::from_rgb(57, 58, 55)
        );
        assert!((ADDRESS_HEIGHT - 46.8).abs() < 0.001);
        assert!((ADDRESS_INPUT_TEXT_SIZE - 18.0).abs() < 0.001);
        assert_eq!(ADDRESS_CORNER_RADIUS, 8);
        assert_eq!(APP_TITLE_WIDTH, 160.0);
        assert_eq!(APP_TITLE_HEIGHT, TAB_STRIP_HEIGHT);
        assert_eq!(APP_TITLE_LEFT_PADDING, 31.0);
        assert_eq!(APP_TITLE_TEXT_SIZE, 28.0);
        assert_eq!(app_title_background_color(), slate_theme::TITLE_SURFACE);
        assert_eq!(app_title_text_color(), egui::Color32::from_rgb(29, 29, 26));
        assert_eq!(TAB_WIDTH, 308.0);
        assert_eq!(TAB_MIN_WIDTH, 196.0);
        assert_eq!(TAB_OPENING_PREFERRED_WIDTH, 244.0);
        assert_eq!(TAB_OPENING_WINDOW_WIDTH, 1024.0);
        assert_eq!(TAB_CONCEPT_WINDOW_WIDTH, 1672.0);
        assert_eq!(tab_content_width(TAB_WIDTH), 276.0);
        assert_eq!(TAB_HEIGHT, 60.0);
        assert_eq!(TAB_CORNER_RADIUS, 8);
        assert_eq!(
            TAB_CONTENT_HEIGHT,
            TAB_HEIGHT - f32::from(TAB_INNER_MARGIN_Y) * 2.0
        );
        assert_eq!(TAB_INNER_MARGIN_X, 16);
        assert_eq!(TAB_INNER_MARGIN_Y, 8);
        assert_eq!(TAB_TITLE_MIN_WIDTH, 80.0);
        assert_eq!(TAB_TITLE_TEXT_SIZE, 20.0);
        assert_eq!(TAB_ICON_TITLE_GAP, 12.0);
        assert_eq!(TAB_TITLE_CLOSE_GAP, 8.0);
        assert_eq!(active_tab_background_color(), slate_theme::SURFACE);
        assert_eq!(active_tab_outline_color(), slate_theme::BORDER);
        assert_eq!(inactive_tab_background_color(), slate_theme::PANEL);
        assert_eq!(
            inactive_tab_hover_background_color(),
            slate_theme::PANEL_HOVER
        );
        assert_eq!(TAB_CLOSE_BUTTON_SIZE, 24.0);
        assert_eq!(TAB_CLOSE_BUTTON_RADIUS, 6);
        assert_eq!(TAB_CLOSE_ICON_SIZE, 12.0);
        assert_eq!(NEW_TAB_LEFT_GAP, 9.0);
        assert_eq!(NEW_TAB_SLOT_HEIGHT, TAB_HEIGHT);
        assert_eq!(NEW_TAB_BUTTON_SIZE, 44.0);
        assert_eq!(NEW_TAB_BUTTON_RADIUS, 8);
        assert_eq!(NEW_TAB_ICON_SIZE, 17.0);
        assert_eq!(NEW_TAB_ICON_STROKE, 2.0);
        assert_eq!(new_tab_icon_color(), slate_theme::TEXT);
        assert_eq!(HOME_SEARCH_MIN_WIDTH, 280.0);
        assert_eq!(HOME_SEARCH_MAX_WIDTH, 880.0);
        assert_eq!(HOME_SEARCH_WIDTH_FACTOR, 0.56);
        assert_eq!(HOME_SEARCH_HORIZONTAL_PADDING, 32.0);
        assert_eq!(HOME_SEARCH_HEIGHT, 72.0);
        assert_eq!(HOME_SEARCH_FRAME_EXTRA_HEIGHT, 8.0);
        assert_eq!(home_search_rendered_height(), 80.0);
        assert_eq!(HOME_SEARCH_TEXT_HEIGHT, 34.0);
        assert_eq!(HOME_SEARCH_INPUT_TEXT_SIZE, 20.0);
        assert_eq!(HOME_SEARCH_INNER_MARGIN_X, 28);
        assert_eq!(HOME_SEARCH_ICON_SIZE, 40.0);
        assert_eq!(HOME_SEARCH_ICON_OFFSET_Y, -3.0);
        assert_eq!(
            home_search_icon_color(),
            egui::Color32::from_rgb(88, 87, 89)
        );
        assert_eq!(HOME_SEARCH_ICON_GAP, 24.0);
        assert_eq!(HOME_SEARCH_CORNER_RADIUS, 8);
        assert_eq!(home_search_background_color(), slate_theme::FIELD_SURFACE);
        assert_eq!(home_search_border_color(), slate_theme::BORDER);
        assert_eq!(TOOLBAR_PANEL_MARGIN_X, 18);
        assert_eq!(TOOLBAR_PANEL_MARGIN_Y, 10);
        assert!((TOOLBAR_ITEM_SPACING - 18.0).abs() < 0.001);
        assert!((TOOLBAR_BUTTON_SIZE - 36.0).abs() < 0.001);
        assert_eq!(TOOLBAR_BUTTON_RADIUS, 8);
        assert!((TOOLBAR_ICON_SIZE - 21.6).abs() < 0.001);
        assert!((TOOLBAR_NAV_ICON_SIZE - 25.2).abs() < 0.001);
        assert!((TOOLBAR_NAV_BACK_ICON_OFFSET_X - 7.2).abs() < 0.001);
        assert!((TOOLBAR_NAV_FORWARD_ICON_OFFSET_X - 6.3).abs() < 0.001);
        assert!((TOOLBAR_NAV_REFRESH_ICON_OFFSET_X - 5.4).abs() < 0.001);
        assert!((TOOLBAR_PRIVACY_ICON_SIZE - 36.0).abs() < 0.001);
        assert!((TOOLBAR_MENU_ICON_WIDTH - 18.0).abs() < 0.001);
        assert!((TOOLBAR_MENU_ICON_OFFSET_X + 2.7).abs() < 0.001);
        assert!((TOOLBAR_MENU_ICON_GAP - 7.65).abs() < 0.001);
        assert!((TOOLBAR_MENU_ICON_STROKE - 1.8).abs() < 0.001);
        assert_eq!(toolbar_menu_icon_color(false), slate_theme::TEXT);
        assert_eq!(toolbar_menu_icon_color(true), slate_theme::TEXT);
        assert!((TOOLBAR_SEPARATOR_HEIGHT - 25.2).abs() < 0.001);
        assert_eq!(
            chrome_vertical_separator_color(),
            egui::Color32::from_rgb(225, 225, 225)
        );
        assert!((TOOLBAR_SEPARATOR_LEADING_GAP - 16.2).abs() < 0.001);
        assert!((TOOLBAR_SEPARATOR_TRAILING_GAP - 19.8).abs() < 0.001);
        assert_eq!(TAB_ICON_SIZE, 32.0);
        assert!((ADDRESS_LEADING_GAP - 18.0).abs() < 0.001);
        assert_eq!(address_background_color(), slate_theme::FIELD_SURFACE);
        assert_eq!(address_border_color(), slate_theme::FIELD_BORDER);
        assert_eq!(ADDRESS_INNER_MARGIN_X, 18);
        assert_eq!(ADDRESS_SHADOW_OFFSET, [0, 1]);
        assert_eq!(ADDRESS_SHADOW_BLUR, 6);
        assert_eq!(ADDRESS_SHADOW_SPREAD, 0);
        assert_eq!(ADDRESS_SHADOW_ALPHA, 6);
        assert!((ADDRESS_SECURITY_ICON_SIZE - 21.6).abs() < 0.001);
        assert!((ADDRESS_SLATE_SECURITY_ICON_SIZE - 30.6).abs() < 0.001);
        assert!((ADDRESS_SLATE_SECURITY_ICON_OFFSET_X + 1.8).abs() < 0.001);
        assert!((ADDRESS_ICON_GAP - 12.6).abs() < 0.001);
        assert!((ADDRESS_BOOKMARK_ICON_SIZE - 19.8).abs() < 0.001);
        assert_eq!(
            address_passive_icon_color(),
            egui::Color32::from_rgb(84, 84, 84)
        );
        assert_eq!(address_bookmark_icon_color(), address_passive_icon_color());
        assert!((ADDRESS_BOOKMARK_BUTTON_SIZE - 25.2).abs() < 0.001);
        assert_eq!(ADDRESS_BOOKMARK_BUTTON_RADIUS, 6);
        assert!((ADDRESS_BOOKMARK_RESERVED_WIDTH - 25.2).abs() < 0.001);
        assert!((ADDRESS_TRAILING_CONTROLS_WIDTH - 169.2).abs() < 0.001);
        assert!((ADDRESS_TRAILING_GAP - 5.4).abs() < 0.001);
        assert_eq!(HOME_TOP_SPACE_FACTOR, 0.18);
        assert_eq!(HOME_TOP_SPACE_MIN, 48.0);
        assert_eq!(HOME_TOP_SPACE_MAX, 132.0);
        assert_eq!(HOME_BOTTOM_MIN_GAP, 16.0);
        assert_eq!(HOME_HERO_SIZE, 78.0);
        assert_eq!(HOME_MOTTO_WIDTH, 280.0);
        assert_eq!(HOME_MOTTO_HEIGHT, 28.0);
        assert_eq!(HOME_MOTTO_TEXT_SIZE, 20.0);
        assert_eq!(HOME_HERO_MOTTO_GAP, 14.0);
        assert_eq!(HOME_HERO_TO_SEARCH_GAP, 41.0);
        assert_eq!(HOME_SEARCH_TO_METRICS_GAP, 57.0);
        assert_eq!(home_view_background_color(), slate_theme::HOME_BG);
        assert_eq!(home_metric_card_background_color(), slate_theme::HOME_BG);
        assert_eq!(HOME_PANEL_SHADOW_OFFSET, [0, 2]);
        assert_eq!(HOME_PANEL_SHADOW_BLUR, 12);
        assert_eq!(HOME_PANEL_SHADOW_SPREAD, 0);
        assert_eq!(HOME_PANEL_SHADOW_ALPHA, 6);
        assert_eq!(HOME_METRIC_CARD_HEIGHT, 172.0);
        assert_eq!(HOME_METRIC_GRID_EXTRA_HEIGHT, 25.0);
        assert_eq!(home_metrics_rendered_height(), 197.0);
        assert_eq!(HOME_METRIC_CARD_MAX_WIDTH, 194.0);
        assert_eq!(HOME_METRIC_CARD_GAP, 33.0);
        assert_eq!(HOME_METRIC_CARD_INNER_MARGIN_X, 16);
        assert_eq!(HOME_METRIC_CARD_INNER_MARGIN_Y, 36);
        assert_eq!(HOME_METRIC_ICON_SIZE, 52.0);
        assert_eq!(HOME_METRIC_ICON_LABEL_GAP, 16.0);
        assert_eq!(HOME_METRIC_LABEL_TEXT_SIZE, 16.0);
        assert_eq!(HOME_METRIC_DETAIL_TEXT_SIZE, 13.0);
        assert_eq!(
            home_metric_detail_color(),
            egui::Color32::from_rgb(145, 144, 144)
        );
        assert_eq!(HOME_METRIC_DETAIL_GAP, 4.0);
        assert_eq!(HOME_METRIC_BADGE_TEXT_SIZE, 13.0);
        assert_eq!(HOME_METRIC_BADGE_PRIMARY_DIGIT_FACTOR, 0.58);
        assert_eq!(HOME_METRIC_BADGE_EXTRA_DIGIT_FACTOR, 0.31);
        assert_eq!(HOME_METRIC_BADGE_LABEL_GAP, 8.0);
        assert_eq!(HOME_METRIC_BADGE_MARGIN_X, 8);
        assert_eq!(HOME_METRIC_BADGE_MARGIN_Y, 3);
        assert_eq!(HOME_METRIC_BADGE_CORNER_RADIUS, 10);
        assert_eq!(HOME_CONTENT_OPTICAL_OFFSET_X, -13.0);
        assert_eq!(HOME_HERO_OPTICAL_OFFSET_X, -29.0);
        assert!((home_metric_badge_width("23") - 31.08).abs() < 0.001);
        assert!((home_metric_badge_width("184") - 35.11).abs() < 0.001);
        assert_eq!(STATUS_BUBBLE_MARGIN_X, 14.0);
        assert_eq!(STATUS_BUBBLE_MARGIN_Y, 12.0);
        assert_eq!(STATUS_BUBBLE_HEIGHT, 32.0);
        assert_eq!(STATUS_BUBBLE_MAX_WIDTH, 560.0);
        assert_eq!(STATUS_BUBBLE_HORIZONTAL_PADDING, 12.0);
        assert_eq!(STATUS_BUBBLE_CORNER_RADIUS, 8);
        assert_eq!(STATUS_BUBBLE_SHADOW_ALPHA, 8);
        assert_eq!(STATUS_TEXT_SIZE, 13.0);
    }

    #[test]
    fn chrome_element_zoom_is_read_from_internal_settings_url() {
        assert_eq!(
            chrome_element_zoom_from_settings_url(
                &Url::parse("slate://settings?chrome_zoom=0.82").unwrap()
            ),
            Some(0.82)
        );
        assert_eq!(
            chrome_element_zoom_from_settings_url(
                &Url::parse("slate://settings?chrome_zoom=0.10").unwrap()
            ),
            Some(CHROME_ELEMENT_ZOOM_MIN)
        );
        assert_eq!(
            chrome_element_zoom_from_settings_url(
                &Url::parse("slate://settings?chrome_zoom=2.00").unwrap()
            ),
            Some(CHROME_ELEMENT_ZOOM_MAX)
        );
        assert_eq!(
            chrome_element_zoom_from_settings_url(
                &Url::parse("slate://settings?other=0.82").unwrap()
            ),
            None
        );
        assert_eq!(
            chrome_element_zoom_from_settings_url(
                &Url::parse("slate://home?chrome_zoom=0.82").unwrap()
            ),
            None
        );
        assert!((clamp_chrome_element_zoom(0.9) - 0.9).abs() < 0.001);
        assert!((chrome_element_zoom_factor(CHROME_ELEMENT_ZOOM) - 1.0).abs() < 0.001);
        assert!(
            (chrome_element_zoom_factor(CHROME_ELEMENT_ZOOM_MAX)
                - CHROME_ELEMENT_ZOOM_MAX / CHROME_ELEMENT_ZOOM)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn full_chrome_geometry_tracks_concept_screenshot_bands() {
        let geometry = concept_chrome_geometry();

        assert_rect_close(
            geometry.tab_strip_rect,
            egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(CONCEPT_SCREENSHOT_WIDTH, TAB_STRIP_HEIGHT),
            ),
        );
        assert_rect_close(
            geometry.app_title_rect,
            egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(APP_TITLE_WIDTH, APP_TITLE_HEIGHT),
            ),
        );
        assert_rect_close(
            geometry.app_rail_rect,
            egui::Rect::from_min_size(
                egui::pos2(0.0, TAB_STRIP_HEIGHT),
                egui::vec2(APP_RAIL_WIDTH, CONCEPT_SCREENSHOT_HEIGHT - TAB_STRIP_HEIGHT),
            ),
        );
        assert_rect_close(
            geometry.toolbar_rect,
            egui::Rect::from_min_size(
                egui::pos2(APP_RAIL_WIDTH, TAB_STRIP_HEIGHT),
                egui::vec2(CONCEPT_SCREENSHOT_WIDTH - APP_RAIL_WIDTH, TOOLBAR_HEIGHT),
            ),
        );
        assert_rect_close(
            geometry.webview_rect,
            egui::Rect::from_min_size(
                egui::pos2(APP_RAIL_WIDTH, TAB_STRIP_HEIGHT + TOOLBAR_HEIGHT),
                egui::vec2(
                    CONCEPT_SCREENSHOT_WIDTH - APP_RAIL_WIDTH,
                    CONCEPT_SCREENSHOT_HEIGHT - TAB_STRIP_HEIGHT - TOOLBAR_HEIGHT - FOOTER_HEIGHT,
                ),
            ),
        );
        assert_rect_close(
            geometry.footer_rect,
            egui::Rect::from_min_size(
                egui::pos2(APP_RAIL_WIDTH, CONCEPT_SCREENSHOT_HEIGHT - FOOTER_HEIGHT),
                egui::vec2(CONCEPT_SCREENSHOT_WIDTH - APP_RAIL_WIDTH, FOOTER_HEIGHT),
            ),
        );
        assert_eq!(
            geometry.webview_rect.size(),
            concept_screenshot_home_view_size()
        );
        assert_eq!(
            geometry.toolbar_content_rect.min,
            egui::pos2(
                APP_RAIL_WIDTH + f32::from(TOOLBAR_PANEL_MARGIN_X),
                TAB_STRIP_HEIGHT + f32::from(TOOLBAR_PANEL_MARGIN_Y)
            )
        );
    }

    #[test]
    fn concept_rail_stack_tracks_reference_icon_spacing() {
        let geometry = concept_chrome_geometry();

        assert_rect_close(
            geometry.rail_button_rects[0],
            egui::Rect::from_min_size(
                egui::pos2(12.0, 97.8),
                egui::vec2(RAIL_BUTTON_SIZE, RAIL_BUTTON_SIZE),
            ),
        );
        assert!(
            (geometry.rail_button_rects[1].center().y
                - geometry.rail_button_rects[0].center().y
                - 82.8)
                .abs()
                < 0.01
        );
        assert!(
            (geometry.rail_button_rects[3].center().y
                - geometry.rail_button_rects[0].center().y
                - 248.4)
                .abs()
                < 0.01
        );
        assert_eq!(RAIL_ICON_SIZE, 36.0);
    }

    #[test]
    fn concept_tab_row_places_new_tab_control_like_reference() {
        let geometry = concept_chrome_geometry();

        assert_eq!(geometry.tab_rects[0].left(), APP_TITLE_WIDTH);
        assert_eq!(geometry.tab_rects[0].top(), TAB_STRIP_HEIGHT - TAB_HEIGHT);
        assert_eq!(geometry.tab_rects[0].width(), TAB_WIDTH);
        assert_eq!(geometry.tab_rects[1].left(), APP_TITLE_WIDTH + TAB_WIDTH);
        assert_eq!(geometry.tab_rects[2].right(), 1084.0);
        assert_eq!(geometry.new_tab_slot_rect.left(), 1093.0);
        assert_eq!(
            geometry.new_tab_button_rect.center(),
            egui::pos2(1115.0, 48.0)
        );
        assert_eq!(NEW_TAB_ICON_SIZE, 17.0);
    }

    #[test]
    fn concept_toolbar_controls_track_reference_positions() {
        let geometry = concept_toolbar_controls_geometry();

        assert_rect_close(
            geometry.nav_button_rects[0],
            egui::Rect::from_min_size(
                egui::pos2(111.6, 97.8),
                egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
            ),
        );
        assert!(points_are_close(
            geometry.nav_button_rects[1].center(),
            egui::pos2(183.6, 115.8)
        ));
        assert!(points_are_close(
            geometry.nav_button_rects[2].center(),
            egui::pos2(237.6, 115.8)
        ));
        assert!(
            (geometry.nav_icon_rects[0].size().x - TOOLBAR_NAV_ICON_SIZE).abs() < 0.01
                && (geometry.nav_icon_rects[0].size().y - TOOLBAR_NAV_ICON_SIZE).abs() < 0.01
        );
        assert_rect_close(
            geometry.address_rect,
            egui::Rect::from_min_size(egui::pos2(291.6, 92.4), egui::vec2(1229.2, ADDRESS_HEIGHT)),
        );
        assert_rect_close(
            geometry.address_security_slot_rect,
            egui::Rect::from_min_size(
                egui::pos2(309.6, 105.0),
                egui::Vec2::splat(ADDRESS_SECURITY_ICON_SIZE),
            ),
        );
        assert_eq!(
            address_slate_security_icon_rect(geometry.address_security_slot_rect),
            geometry.address_slate_security_icon_rect
        );
        let address_security_visible =
            address_slate_security_visible_rect(geometry.address_slate_security_icon_rect);
        assert!(
            (310.0..=312.0).contains(&address_security_visible.left()),
            "expected address shield to track the compact address field: {:?}",
            geometry.address_slate_security_icon_rect
        );
        assert!(
            (105.0..=107.0).contains(&address_security_visible.top()),
            "expected address shield to track the compact address field: {:?}",
            geometry.address_slate_security_icon_rect
        );
        assert!(
            (326.0..=328.0).contains(&address_security_visible.right()),
            "expected address shield to track the compact address field: {:?}",
            geometry.address_slate_security_icon_rect
        );
        assert!(
            (125.0..=127.0).contains(&address_security_visible.bottom()),
            "expected address shield to track the compact address field: {:?}",
            geometry.address_slate_security_icon_rect
        );
        assert_rect_close(
            geometry.address_text_rect,
            egui::Rect::from_min_size(
                egui::pos2(343.8, 100.5),
                egui::vec2(1133.8, ADDRESS_TEXT_HEIGHT),
            ),
        );
        assert!(points_are_close(
            geometry.address_bookmark_button_rect.center(),
            egui::pos2(1490.2, 115.8)
        ));
        assert_eq!(
            address_bookmark_icon_rect(geometry.address_bookmark_button_rect),
            geometry.address_bookmark_icon_rect
        );
        assert!((geometry.address_bookmark_icon_rect.left() - 1480.3).abs() < 0.01);
        assert!((geometry.address_bookmark_icon_rect.right() - 1500.1).abs() < 0.01);
        assert!(points_are_close(
            geometry.privacy_button_rect.center(),
            egui::pos2(1562.2, 115.8)
        ));
        assert!((geometry.separator_rect.center().x - 1596.9).abs() < 0.01);
        assert!((geometry.separator_rect.top() - 103.2).abs() < 0.01);
        assert!((geometry.separator_rect.bottom() - 128.4).abs() < 0.01);
        assert!(points_are_close(
            geometry.menu_button_rect.center(),
            egui::pos2(1635.2, 115.8)
        ));
    }

    #[test]
    fn concept_footer_controls_track_reference_positions() {
        let geometry = concept_footer_controls_geometry();

        assert_rect_close(
            geometry.load_status_rect,
            egui::Rect::from_min_size(
                egui::pos2(109.6, 905.0),
                egui::vec2(1550.4, FOOTER_LOAD_STATUS_HEIGHT),
            ),
        );
        assert!((geometry.load_status_dot_center.x - 114.6).abs() < 0.01);
        assert!((geometry.load_status_dot_center.y - 919.0).abs() < 0.01);
    }

    #[test]
    fn wide_toolbar_address_width_leaves_room_for_trailing_controls() {
        assert!((toolbar_address_width(1332.0) - 1162.8).abs() < 0.01);
        assert!((address_outer_width(toolbar_address_width(1332.0)) - 1198.8).abs() < 0.01);
    }

    #[test]
    fn narrow_toolbar_address_width_stays_within_available_width() {
        assert_eq!(toolbar_address_width(220.0), 220.0);
        assert!(toolbar_address_width(300.0) >= ADDRESS_MIN_WIDTH);
    }

    #[test]
    fn status_bubble_width_is_bounded_and_truncates_long_text() {
        let status =
            "https://example.com/a/very/long/path/that/should/not/stretch/across/the/browser";
        let width = status_bubble_width(status, 1200.0);

        assert_eq!(width, STATUS_BUBBLE_MAX_WIDTH);
        assert!(status_bubble_width(status, 240.0) <= 240.0);

        let label = status_bubble_label(status, 180.0);
        assert!(label.ends_with('…'));
        assert!(label.chars().count() < status.chars().count());
    }

    #[test]
    fn footer_load_status_prefers_broadweb_progress_for_ipfs_locations() {
        let broadweb_status = BroadwebStatusSnapshot {
            kind: BroadwebStatusKind::SwitchingGateway,
            message: "Trying w3s.link".to_string(),
            target: Some("ipfs://bafybeigdyrzt".to_string()),
            gateway: Some("https://w3s.link".to_string()),
            sequence: 4,
        };

        assert_eq!(
            Gui::footer_load_status_label(
                LoadStatus::Started,
                &broadweb_status,
                "ipfs://bafybeigdyrzt",
            ),
            "Trying w3s.link"
        );
        assert_eq!(
            Gui::footer_load_status_label(
                LoadStatus::Started,
                &broadweb_status,
                "ipns://example.ipns",
            ),
            "Trying w3s.link"
        );
        assert_eq!(
            Gui::footer_load_status_label(
                LoadStatus::Started,
                &broadweb_status,
                "https://example.com",
            ),
            "Loading..."
        );
    }

    #[test]
    fn footer_load_status_indicator_pulses_only_while_progress_is_active() {
        let idle_status = BroadwebStatusSnapshot::idle();
        let fetching_status = BroadwebStatusSnapshot {
            kind: BroadwebStatusKind::Fetching,
            message: "Fetching IPFS content".to_string(),
            target: Some("ipfs://bafybeigdyrzt".to_string()),
            gateway: Some("https://w3s.link".to_string()),
            sequence: 2,
        };

        assert!(!footer_load_status_is_in_progress(
            LoadStatus::Complete,
            &idle_status
        ));
        assert!(footer_load_status_is_in_progress(
            LoadStatus::Started,
            &idle_status
        ));
        assert!(footer_load_status_is_in_progress(
            LoadStatus::Complete,
            &fetching_status
        ));
        assert_eq!(
            footer_load_status_indicator_color_at(LoadStatus::Complete, &idle_status, 0.625),
            footer_load_status_indicator_color(LoadStatus::Complete)
        );
        assert_eq!(
            footer_load_status_indicator_color_at(LoadStatus::Started, &idle_status, 0.0),
            footer_load_status_indicator_color(LoadStatus::Started)
        );
        assert_eq!(
            footer_load_status_indicator_color_at(LoadStatus::Started, &idle_status, 0.625),
            footer_load_status_pulse_target_color()
        );
        assert_eq!(
            footer_load_status_indicator_color_at(LoadStatus::Complete, &fetching_status, 0.0),
            footer_load_status_indicator_color(LoadStatus::Started)
        );
    }

    #[test]
    fn address_security_icon_uses_slate_shield_for_home() {
        assert_eq!(
            address_security_icon_for_location("slate://home"),
            AddressSecurityIcon::Slate {
                icon: slate_theme::SlateIcon::TopShield,
                color: address_passive_icon_color(),
            }
        );
        assert_eq!(
            address_security_icon_for_location("slate://web"),
            AddressSecurityIcon::Slate {
                icon: slate_theme::SlateIcon::TopShield,
                color: address_passive_icon_color(),
            }
        );
        assert_eq!(
            address_security_icon_for_location("slate://settings"),
            AddressSecurityIcon::Slate {
                icon: slate_theme::SlateIcon::TopShield,
                color: address_passive_icon_color(),
            }
        );
    }

    #[test]
    fn address_security_icon_reflects_common_url_schemes() {
        assert_eq!(
            address_security_icon_for_location(""),
            AddressSecurityIcon::Raster(slate_theme::SlateRaster::Search)
        );
        assert_eq!(
            address_security_icon_for_location("https://example.com"),
            AddressSecurityIcon::Raster(slate_theme::SlateRaster::PageInfoSecure)
        );
        assert_eq!(
            address_security_icon_for_location("http://example.com"),
            AddressSecurityIcon::Raster(slate_theme::SlateRaster::PageInfoInsecure)
        );
        assert_eq!(
            address_security_icon_for_location("file:///tmp/index.html"),
            AddressSecurityIcon::Raster(slate_theme::SlateRaster::PageInfoLocal)
        );
        assert_eq!(
            address_security_icon_for_location("resource://servo/user-agent.css"),
            AddressSecurityIcon::Raster(slate_theme::SlateRaster::PageInfoInternal)
        );
        assert_eq!(
            address_security_icon_for_location("not a url yet"),
            AddressSecurityIcon::Raster(slate_theme::SlateRaster::PageInfoWarning)
        );
    }

    #[test]
    fn address_security_raster_colors_match_chrome_state() {
        assert_eq!(
            address_security_raster_color(slate_theme::SlateRaster::PageInfoSecure),
            address_passive_icon_color()
        );
        assert_eq!(
            address_security_raster_color(slate_theme::SlateRaster::PageInfoLocal),
            address_passive_icon_color()
        );
        assert_eq!(
            address_security_raster_color(slate_theme::SlateRaster::PageInfoInternal),
            address_passive_icon_color()
        );
        assert_eq!(
            address_security_raster_color(slate_theme::SlateRaster::PageInfoInsecure),
            slate_theme::AMBER
        );
        assert_eq!(
            address_security_raster_color(slate_theme::SlateRaster::PageInfoWarning),
            slate_theme::AMBER
        );
    }

    #[test]
    fn home_top_spacing_is_responsive_with_bounds() {
        assert_eq!(home_top_space(120.0), 0.0);
        assert!(
            home_content_stack_height(default_opening_home_view_height())
                <= default_opening_home_view_height()
        );
        assert!((home_top_space(700.0) - 126.0).abs() < 0.001);
        assert_eq!(home_top_space(900.0), HOME_TOP_SPACE_MAX);
    }

    #[test]
    fn home_search_width_uses_padding_and_bounds() {
        assert!((home_search_width(1672.0 - APP_RAIL_WIDTH) - 880.0).abs() < 0.01);
        assert!((home_search_width(1200.0) - 672.0).abs() < 0.01);
        assert!((home_search_width(620.0) - 347.2).abs() < 0.01);
        assert_eq!(home_search_width(250.0), 218.0);
    }

    #[test]
    fn home_content_left_space_applies_concept_optical_offset() {
        let viewport_width = concept_screenshot_home_view_size().x;
        let search_width = home_search_width(viewport_width);

        assert!((home_content_left_space(viewport_width, search_width) - 336.2).abs() < 0.01);
        assert!((home_hero_left_space(viewport_width, HOME_HERO_SIZE) - 721.2).abs() < 0.01);
        assert_eq!(home_content_left_space(220.0, 260.0), 0.0);
        assert_eq!(home_hero_left_space(220.0, 260.0), 0.0);
        assert_eq!(home_content_left_space(100.0, 100.0), 0.0);
    }

    #[test]
    fn home_search_icon_rect_tracks_reference_glyph_offset() {
        let slot_rect =
            egui::Rect::from_min_size(egui::pos2(360.0, 263.8), egui::Vec2::splat(40.0));
        let icon_rect = home_search_icon_rect(slot_rect);
        let visible_rect = home_search_icon_visible_rect(icon_rect);

        assert!((icon_rect.center().x - 380.0).abs() < 0.01);
        assert!((icon_rect.center().y - 280.8).abs() < 0.01);
        assert!(
            (368.0..=370.0).contains(&visible_rect.left()),
            "expected visible search icon to start near screenshot x=473 absolute: {visible_rect:?}"
        );
        assert!(
            (267.0..=269.0).contains(&visible_rect.top()),
            "expected visible search icon to start near screenshot y=430 absolute: {visible_rect:?}"
        );
    }

    #[test]
    fn tab_title_width_reserves_fixed_close_region() {
        assert_eq!(tab_title_width(tab_content_width(TAB_WIDTH)), 200.0);
        assert_eq!(tab_title_width(100.0), TAB_TITLE_MIN_WIDTH);
    }

    #[test]
    fn tab_icon_and_title_slots_match_reference_spacing() {
        let geometry = concept_chrome_geometry();
        let first_icon_slot = tab_icon_slot_rect(geometry.tab_rects[0]);
        let second_icon_slot = tab_icon_slot_rect(geometry.tab_rects[1]);
        let third_icon_slot = tab_icon_slot_rect(geometry.tab_rects[2]);

        assert_eq!(first_icon_slot.size(), egui::Vec2::splat(TAB_ICON_SIZE));
        assert_eq!(first_icon_slot.left(), 176.0);
        assert_eq!(first_icon_slot.top(), 32.0);
        assert_eq!(tab_title_left(geometry.tab_rects[0]), 220.0);
        assert_eq!(tab_title_left(geometry.tab_rects[1]), 528.0);
        assert_eq!(tab_title_left(geometry.tab_rects[2]), 836.0);
        assert_eq!(
            second_icon_slot.center().x - first_icon_slot.center().x,
            TAB_WIDTH
        );
        assert_eq!(
            third_icon_slot.center().x - second_icon_slot.center().x,
            TAB_WIDTH
        );
    }

    #[test]
    fn tab_close_button_tracks_reference_right_edge_offset() {
        let tab_rect = concept_chrome_geometry().tab_rects[0];
        let close_rect = tab_close_button_rect(tab_rect);

        assert_eq!(close_rect.size(), egui::Vec2::splat(TAB_CLOSE_BUTTON_SIZE));
        assert_eq!(close_rect.center().x, tab_rect.right() - 28.0);
        assert_eq!(close_rect.center().y, tab_rect.center().y);
    }

    #[test]
    fn tab_width_tracks_window_proportions() {
        assert_eq!(
            tab_width_for_strip(TAB_OPENING_WINDOW_WIDTH - APP_TITLE_WIDTH, 1),
            TAB_OPENING_PREFERRED_WIDTH
        );
        assert_eq!(
            tab_width_for_strip(TAB_CONCEPT_WINDOW_WIDTH - APP_TITLE_WIDTH, 3),
            TAB_WIDTH
        );
        assert_eq!(
            tab_width_for_strip(TAB_OPENING_WINDOW_WIDTH - APP_TITLE_WIDTH, 8),
            TAB_MIN_WIDTH
        );
    }

    #[test]
    fn active_tab_separator_join_breaks_divider_under_selected_tab() {
        let strip_rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1672.0, TAB_STRIP_HEIGHT));
        let active_tab_rect = egui::Rect::from_min_size(
            egui::pos2(APP_TITLE_WIDTH, TAB_STRIP_HEIGHT - TAB_HEIGHT),
            egui::vec2(TAB_WIDTH, TAB_HEIGHT),
        );
        let join = active_tab_separator_join(strip_rect, active_tab_rect)
            .expect("active tab should create a divider join");
        let divider_y = strip_rect.bottom() - 0.5;

        assert_eq!(join.tab_left, active_tab_rect.left());
        assert_eq!(join.tab_right, active_tab_rect.right());
        assert_eq!(join.separator_y, divider_y);
        assert_eq!(
            join.bridge_rect.left(),
            active_tab_rect.left() + ACTIVE_TAB_BOTTOM_JOIN_INSET_X
        );
        assert_eq!(
            join.bridge_rect.right(),
            active_tab_rect.right() - ACTIVE_TAB_BOTTOM_JOIN_INSET_X
        );
        assert!(join.bridge_rect.top() < divider_y);
        assert!(join.bridge_rect.bottom() > divider_y);
    }

    #[test]
    fn active_tab_separator_routes_around_selected_file_tab() {
        let strip_rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1672.0, TAB_STRIP_HEIGHT));
        let active_tab_rect = egui::Rect::from_min_size(
            egui::pos2(APP_TITLE_WIDTH, TAB_STRIP_HEIGHT - TAB_HEIGHT),
            egui::vec2(TAB_WIDTH, TAB_HEIGHT),
        );
        let (join, divider_points) = active_tab_content_divider_points(strip_rect, active_tab_rect)
            .expect("active tab should route the content divider");
        let topmost_y = divider_points
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);

        assert_eq!(
            divider_points.first().copied(),
            Some(egui::pos2(strip_rect.left(), join.separator_y))
        );
        assert_eq!(
            divider_points.last().copied(),
            Some(egui::pos2(strip_rect.right(), join.separator_y))
        );
        assert!(
            (topmost_y - active_tab_rect.top()).abs() < 0.01,
            "expected active tab divider to reach tab top: {divider_points:?}"
        );
        assert!(
            divider_points.windows(2).any(|segment| {
                points_are_close(segment[0], egui::pos2(strip_rect.left(), join.separator_y))
                    && points_are_close(
                        segment[1],
                        egui::pos2(active_tab_rect.left(), join.separator_y),
                    )
            }),
            "expected divider to approach the active tab from the horizontal strip: {divider_points:?}"
        );
        assert!(
            divider_points.iter().any(|point| {
                (point.x - active_tab_rect.left()).abs() < 0.01
                    && point.y < join.separator_y - TAB_HEIGHT / 2.0
            }),
            "expected separator to climb the active tab's left edge: {divider_points:?}"
        );
        assert!(
            divider_points.windows(2).any(|segment| {
                (segment[0].y - active_tab_rect.top()).abs() < 0.01
                    && (segment[1].y - active_tab_rect.top()).abs() < 0.01
                    && segment[1].x > segment[0].x
            }),
            "expected separator to run across the active tab's top label edge: {divider_points:?}"
        );
        assert!(
            divider_points.iter().any(|point| {
                (point.x - active_tab_rect.right()).abs() < 0.01
                    && point.y < join.separator_y - TAB_HEIGHT / 2.0
            }),
            "expected separator to descend the active tab's right edge: {divider_points:?}"
        );
        assert!(
            divider_points.windows(2).any(|segment| {
                points_are_close(
                    segment[0],
                    egui::pos2(active_tab_rect.right(), join.separator_y),
                ) && points_are_close(segment[1], egui::pos2(strip_rect.right(), join.separator_y))
            }),
            "expected divider to continue horizontally after the active tab: {divider_points:?}"
        );
    }

    #[test]
    fn active_tab_divider_uses_visible_edges_when_tab_is_clipped() {
        let strip_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(500.0, TAB_STRIP_HEIGHT));
        let active_tab_rect = egui::Rect::from_min_size(
            egui::pos2(50.0, TAB_STRIP_HEIGHT - TAB_HEIGHT),
            egui::vec2(TAB_WIDTH, TAB_HEIGHT),
        );
        let (join, divider_points) = active_tab_content_divider_points(strip_rect, active_tab_rect)
            .expect("partially visible active tab should route the divider");

        assert_eq!(join.tab_left, strip_rect.left());
        assert_eq!(join.tab_right, active_tab_rect.right());
        assert!(
            divider_points
                .iter()
                .all(|point| point.x >= strip_rect.left() && point.x <= strip_rect.right()),
            "expected divider points to stay inside visible strip: {divider_points:?}"
        );
        assert!(
            divider_points.iter().any(|point| {
                (point.x - strip_rect.left()).abs() < 0.01
                    && point.y < join.separator_y - TAB_HEIGHT / 2.0
            }),
            "expected clipped active tab divider to climb from the visible edge: {divider_points:?}"
        );
    }

    #[test]
    fn inactive_tab_outline_uses_open_file_tab_path() {
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(APP_TITLE_WIDTH + TAB_WIDTH, TAB_STRIP_HEIGHT - TAB_HEIGHT),
            egui::vec2(TAB_WIDTH, TAB_HEIGHT),
        );
        let outline_points = inactive_tab_outline_points(tab_rect);
        let bottom_y = tab_rect.bottom() - 0.5;
        let topmost_y = outline_points
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);

        assert_eq!(inactive_tab_outline_color(), slate_theme::BORDER);
        assert_eq!(
            outline_points.first().copied(),
            Some(egui::pos2(tab_rect.left(), bottom_y))
        );
        assert_eq!(
            outline_points.last().copied(),
            Some(egui::pos2(tab_rect.right(), bottom_y))
        );
        assert!(
            (topmost_y - tab_rect.top()).abs() < 0.01,
            "expected inactive tab outline to climb to tab top: {outline_points:?}"
        );
        assert!(
            !outline_points.windows(2).any(|segment| {
                (segment[0].y - bottom_y).abs() < 0.01
                    && (segment[1].y - bottom_y).abs() < 0.01
                    && (segment[1].x - segment[0].x).abs() > 1.0
            }),
            "inactive tab outline should leave the bottom edge to the shared strip divider: {outline_points:?}"
        );
    }

    #[test]
    fn tab_chrome_colors_keep_file_tab_content_readable() {
        assert_eq!(active_tab_background_color(), slate_theme::SURFACE);
        assert_eq!(active_tab_outline_color(), slate_theme::BORDER);
        assert_eq!(inactive_tab_background_color(), slate_theme::PANEL);
        assert_eq!(
            inactive_tab_hover_background_color(),
            slate_theme::PANEL_HOVER
        );
        assert_eq!(inactive_tab_outline_color(), slate_theme::BORDER);
        assert_eq!(tab_title_color(true), slate_theme::TEXT);
        assert_eq!(tab_title_color(false), slate_theme::TEXT);
        assert_eq!(tab_icon_color(true), slate_theme::TEXT);
        assert_eq!(tab_icon_color(false), slate_theme::TEXT);
        assert_eq!(tab_close_icon_color(true), slate_theme::TEXT);
        assert_eq!(tab_close_icon_color(false), slate_theme::TEXT);
        assert_eq!(tab_close_raster(true), slate_theme::SlateRaster::TabClose);
        assert_eq!(tab_close_raster(false), slate_theme::SlateRaster::TabClose);
    }

    #[test]
    fn toolbar_navigation_colors_keep_reference_masks_visible_when_disabled() {
        assert_eq!(toolbar_navigation_icon_color(true), slate_theme::TEXT);
        assert_eq!(toolbar_navigation_icon_color(false), slate_theme::TEXT);
    }

    #[test]
    fn toolbar_navigation_uses_concept_raster_crops() {
        assert_eq!(
            toolbar_navigation_raster(slate_theme::SlateIcon::NavBack, false),
            Some(slate_theme::SlateRaster::NavBack)
        );
        assert_eq!(
            toolbar_navigation_raster(slate_theme::SlateIcon::NavBack, true),
            Some(slate_theme::SlateRaster::NavBackHover)
        );
        assert_eq!(
            toolbar_navigation_raster(slate_theme::SlateIcon::NavForward, false),
            Some(slate_theme::SlateRaster::NavForward)
        );
        assert_eq!(
            toolbar_navigation_raster(slate_theme::SlateIcon::NavForward, true),
            Some(slate_theme::SlateRaster::NavForwardHover)
        );
        assert_eq!(
            toolbar_navigation_raster(slate_theme::SlateIcon::NavRefresh, false),
            Some(slate_theme::SlateRaster::NavReload)
        );
        assert_eq!(
            toolbar_navigation_raster(slate_theme::SlateIcon::NavRefresh, true),
            Some(slate_theme::SlateRaster::NavReloadHover)
        );
    }

    #[test]
    fn toolbar_navigation_icon_offsets_align_reference_masks() {
        fn projected_mask_center_x(button_center_x: f32, icon: slate_theme::SlateIcon) -> f32 {
            let button_rect = egui::Rect::from_center_size(
                egui::pos2(button_center_x, 120.0),
                egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
            );
            toolbar_navigation_icon_rect(button_rect, icon).center().x
        }

        assert_eq!(
            toolbar_navigation_icon_offset_x(slate_theme::SlateIcon::NavBack),
            TOOLBAR_NAV_BACK_ICON_OFFSET_X
        );
        assert_eq!(
            toolbar_navigation_icon_offset_x(slate_theme::SlateIcon::NavForward),
            TOOLBAR_NAV_FORWARD_ICON_OFFSET_X
        );
        assert_eq!(
            toolbar_navigation_icon_offset_x(slate_theme::SlateIcon::NavRefresh),
            TOOLBAR_NAV_REFRESH_ICON_OFFSET_X
        );
        let nav_icon_size = toolbar_navigation_icon_rect(
            egui::Rect::from_center_size(
                egui::pos2(142.0, 120.0),
                egui::Vec2::splat(TOOLBAR_BUTTON_SIZE),
            ),
            slate_theme::SlateIcon::NavBack,
        )
        .size();
        assert!((nav_icon_size.x - TOOLBAR_NAV_ICON_SIZE).abs() < 0.01);
        assert!((nav_icon_size.y - TOOLBAR_NAV_ICON_SIZE).abs() < 0.01);
        assert!(
            (projected_mask_center_x(129.6, slate_theme::SlateIcon::NavBack) - 136.8).abs() < 0.01
        );
        assert!(
            (projected_mask_center_x(183.6, slate_theme::SlateIcon::NavForward) - 189.9).abs()
                < 0.01
        );
        assert!(
            (projected_mask_center_x(237.6, slate_theme::SlateIcon::NavRefresh) - 243.0).abs()
                < 0.01
        );
    }

    #[test]
    fn toolbar_menu_icon_geometry_matches_reference_glyph() {
        let button_rect =
            egui::Rect::from_center_size(egui::pos2(1635.2, 115.8), egui::Vec2::splat(40.0));
        let icon_rect = toolbar_menu_icon_rect(button_rect);

        assert!((icon_rect.width() - 19.8).abs() < 0.01);
        assert!((icon_rect.height() - 17.1).abs() < 0.01);
        assert!(points_are_close(
            toolbar_menu_icon_center(button_rect),
            egui::pos2(1632.5, 115.8)
        ));
        assert!(points_are_close(
            icon_rect.center(),
            egui::pos2(1632.5, 115.8)
        ));
    }

    #[test]
    fn rail_colors_keep_selected_tile_soft() {
        assert_eq!(rail_icon_color(true), slate_theme::TEAL);
        assert_eq!(rail_icon_color(false), slate_theme::TEXT);
        assert_eq!(
            rail_selected_button_fill(),
            egui::Color32::from_rgb(236, 240, 239)
        );
        assert_eq!(rail_button_fill(true, false), rail_selected_button_fill());
        assert_eq!(rail_button_fill(true, true), rail_selected_button_fill());
        assert_eq!(rail_button_fill(false, true), slate_theme::PANEL_HOVER);
        assert_eq!(rail_button_fill(false, false), egui::Color32::TRANSPARENT);
    }

    #[test]
    fn tab_corner_radius_attaches_tabs_to_strip() {
        let radius = tab_corner_radius();

        assert_eq!(radius.nw, TAB_CORNER_RADIUS);
        assert_eq!(radius.ne, TAB_CORNER_RADIUS);
        assert_eq!(radius.sw, 0);
        assert_eq!(radius.se, 0);
    }

    #[test]
    fn wide_home_metrics_layout_matches_concept_width() {
        let layout = home_metrics_layout(880.0);

        assert_eq!(layout.columns, 4);
        assert_eq!(layout.card_width, HOME_METRIC_CARD_MAX_WIDTH);
        assert_eq!(layout.spacing, HOME_METRIC_CARD_GAP);
        assert_eq!(home_metrics_row_width(layout), 875.0);
        assert!(home_metrics_row_width(layout) <= 880.0);
    }

    #[test]
    fn medium_home_metrics_layout_uses_two_columns() {
        let layout = home_metrics_layout(620.0);

        assert_eq!(layout.columns, 2);
        assert_eq!(layout.card_width, HOME_METRIC_CARD_MAX_WIDTH);
        assert_eq!(layout.spacing, HOME_METRIC_CARD_GAP);
        assert!(home_metrics_row_width(layout) <= 620.0);
    }

    #[test]
    fn narrow_home_metrics_layout_stays_within_available_width() {
        let layout = home_metrics_layout(130.0);

        assert_eq!(layout.columns, 1);
        assert_eq!(layout.card_width, 130.0);
        assert!(layout.card_width <= HOME_METRIC_CARD_MIN_WIDTH);
    }

    #[test]
    fn home_metric_card_inner_size_keeps_outer_footprint_stable() {
        assert_eq!(
            home_metric_card_content_width(HOME_METRIC_CARD_MAX_WIDTH),
            162.0
        );
        assert_eq!(home_metric_card_content_height(), 100.0);
    }

    #[test]
    fn default_home_bookmarks_fill_first_run_slots() {
        let bookmarks = default_home_bookmark_cards();

        assert_eq!(bookmarks.len(), 4);
        assert_eq!(bookmarks[0].label, "Wikipedia on IPFS");
        assert_eq!(
            bookmarks[0].url.as_deref(),
            Some("ipns://en.wikipedia-on-ipfs.org/wiki/")
        );
        assert_eq!(
            bookmarks[0].favicon_key.as_deref(),
            Some("favicon:ipns://en.wikipedia-on-ipfs.org/favicon.ico")
        );
        assert_eq!(
            bookmarks[0].favicon_url.as_deref(),
            Some("ipns://en.wikipedia-on-ipfs.org/favicon.ico")
        );
        assert_eq!(bookmarks[1].label, "OpenStreetMap");
        assert_eq!(
            bookmarks[1].url.as_deref(),
            Some("https://www.openstreetmap.org/")
        );
        assert_eq!(
            bookmarks[1].favicon_key.as_deref(),
            Some("favicon:https://www.openstreetmap.org/favicon.ico")
        );
        assert_eq!(
            bookmarks[1].favicon_url.as_deref(),
            Some("https://www.openstreetmap.org/favicon.ico")
        );
        assert_eq!(bookmarks[2].label, "Add bookmark");
        assert!(bookmarks[2].url.is_none());
        assert!(bookmarks[2].favicon_key.is_none());
        assert!(bookmarks[2].favicon_url.is_none());
        assert_eq!(bookmarks[3].label, "Add another");
        assert!(bookmarks[3].url.is_none());
        assert!(bookmarks[3].favicon_key.is_none());
        assert!(bookmarks[3].favicon_url.is_none());
    }

    #[test]
    fn home_bookmark_slot_replaces_default_suggestions_before_user_bookmarks() {
        let bookmarks = vec![
            bookmark_record(DEFAULT_HOME_BOOKMARKS[0].url, 0),
            bookmark_record(DEFAULT_HOME_BOOKMARKS[1].url, 1),
        ];

        assert_eq!(
            home_bookmark_slot_for_url(&bookmarks, "https://example.com/"),
            0
        );

        let bookmarks = vec![
            bookmark_record("https://example.com/", 0),
            bookmark_record(DEFAULT_HOME_BOOKMARKS[1].url, 1),
        ];

        assert_eq!(
            home_bookmark_slot_for_url(&bookmarks, "https://servo.org/"),
            1
        );
    }

    #[test]
    fn home_bookmark_slot_updates_existing_or_replaces_second_user_slot() {
        let bookmarks = vec![
            bookmark_record("https://example.com/", 0),
            bookmark_record("https://servo.org/", 1),
        ];

        assert_eq!(
            home_bookmark_slot_for_url(&bookmarks, "https://example.com/"),
            0
        );
        assert_eq!(
            home_bookmark_slot_for_url(&bookmarks, "https://rust-lang.org/"),
            1
        );
    }

    #[test]
    fn home_bookmark_title_and_scheme_filter_keep_home_slots_user_facing() {
        assert_eq!(
            home_bookmark_title(Some("Example".to_string()), "https://example.com/"),
            "Example"
        );
        assert_eq!(
            home_bookmark_title(Some(String::new()), "ipns://en.wikipedia-on-ipfs.org/wiki/"),
            "ipns://en.wikipedia-on-ipfs.org"
        );

        assert!(is_home_bookmarkable_url("https://example.com/"));
        assert!(is_home_bookmarkable_url("ipfs://bafybeigdyrzt/"));
        assert!(is_home_bookmarkable_url("gemini://example.com/"));
        assert!(!is_home_bookmarkable_url("slate://home"));
        assert!(!is_home_bookmarkable_url("file:///tmp/index.html"));
        assert!(!is_home_bookmarkable_url("resource://servo/user-agent.css"));
        assert!(!is_home_bookmarkable_url("servo://resources/prefs"));
    }

    #[test]
    fn home_bookmark_favicon_cache_key_and_url_are_deterministic() {
        assert_eq!(
            home_bookmark_favicon_key("https://example.com/path/page.html"),
            "favicon:https://example.com/favicon.ico"
        );
        assert_eq!(
            home_bookmark_favicon_url("https://example.com/path/page.html").as_deref(),
            Some("https://example.com/favicon.ico")
        );
        assert_eq!(
            home_bookmark_favicon_url("ipfs://bafybeigdyrzt/site/index.html").as_deref(),
            Some("ipfs://bafybeigdyrzt/favicon.ico")
        );
        assert_eq!(home_bookmark_favicon_url("gemini://example.com/"), None);
        assert_eq!(home_bookmark_favicon_url("slate://home"), None);
    }

    #[test]
    fn home_favicon_decoder_accepts_small_raster_icons_and_rejects_bad_input() {
        let image = home_favicon_color_image(include_bytes!(
            "../../assets/icons/slate-ns/hotlist-add.png"
        ))
        .expect("decode png favicon");
        assert_eq!(image.size, [17, 17]);
        assert!(home_favicon_color_image(&[]).is_none());
        assert!(home_favicon_color_image(&vec![0; HOME_FAVICON_MAX_BYTES + 1]).is_none());
    }

    #[test]
    fn headless_home_content_fits_default_opening_view() {
        let viewport_size = default_opening_home_view_size();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size);
        let layout = render_home_content_layout(viewport_size);

        for rect in [
            layout.hero_rect,
            layout.motto_rect,
            layout.search_rect,
            layout.search_icon_rect,
            layout.metrics_rect,
        ] {
            assert!(rect_has_area(rect), "expected visible rect: {rect:?}");
            assert!(
                rect_is_inside(bounds, rect),
                "expected {rect:?} to fit inside {bounds:?}"
            );
        }
    }

    #[test]
    fn headless_home_content_tracks_concept_screenshot_geometry() {
        let viewport_size = concept_screenshot_home_view_size();
        let layout = render_home_content_layout(viewport_size);
        let concept_center_x = viewport_size.x / 2.0 + HOME_CONTENT_OPTICAL_OFFSET_X;
        let hero_center_x = viewport_size.x / 2.0 + HOME_HERO_OPTICAL_OFFSET_X;

        assert!(
            (layout.search_rect.center().x - concept_center_x).abs() < LAYOUT_EPSILON + 0.25,
            "expected search to track the concept optical center in {viewport_size:?}: {:?}",
            layout.search_rect
        );
        assert!(
            (layout.hero_rect.center().x - hero_center_x).abs() < LAYOUT_EPSILON + 0.25,
            "expected hero to track its screenshot optical center in {viewport_size:?}: {:?}",
            layout.hero_rect
        );
        assert!(
            (layout.motto_rect.center().x - concept_center_x).abs() < LAYOUT_EPSILON + 0.25,
            "expected motto to track the concept optical center in {viewport_size:?}: {:?}",
            layout.motto_rect
        );
        let hero_visible_rect = home_hero_icon_visible_rect(layout.hero_rect);
        assert!(
            (759.0..=761.0).contains(&hero_visible_rect.center().x),
            "expected hero shield center near screenshot x=860 absolute position: {hero_visible_rect:?}"
        );
        assert!(
            (335.0..=337.0).contains(&layout.search_rect.left()),
            "expected search field to start near the screenshot x=436 absolute position: {:?}",
            layout.search_rect
        );
        let search_icon_visible_rect = home_search_icon_visible_rect(layout.search_icon_rect);
        assert!(
            (372.0..=374.0).contains(&search_icon_visible_rect.left()),
            "expected search icon to start near the screenshot x=473 absolute position: {:?}",
            search_icon_visible_rect
        );
        assert!(
            (319.0..=322.0).contains(&search_icon_visible_rect.top()),
            "expected search icon to track the moved home search field: {:?}",
            search_icon_visible_rect
        );
        assert!(
            (860.0..=900.0).contains(&layout.search_rect.width()),
            "expected search width to stay close to the screenshot max width: {:?}",
            layout.search_rect
        );
        assert!(
            (118.0..=132.0).contains(&layout.hero_rect.top()),
            "expected hero to sit near the screenshot vertical rhythm: {:?}",
            layout.hero_rect
        );
        assert!(
            (226.0..=228.0).contains(&layout.motto_rect.top()),
            "expected motto to sit below the home shield: {:?}",
            layout.motto_rect
        );
        assert!(
            (299.0..=301.0).contains(&layout.search_rect.top()),
            "expected search to sit near the screenshot vertical rhythm: {:?}",
            layout.search_rect
        );
        assert!(
            (432.0..=434.0).contains(&layout.metrics_rect.top()),
            "expected metric cards to sit near the screenshot vertical rhythm: {:?}",
            layout.metrics_rect
        );
        assert!(
            (338.0..=340.0).contains(&layout.metrics_rect.left()),
            "expected metric cards to start near the screenshot horizontal rhythm: {:?}",
            layout.metrics_rect
        );
        assert!(
            (882.0..=884.0).contains(&layout.metrics_rect.width()),
            "expected metric row response to include the screenshot card footprint plus frame shadow: {:?}",
            layout.metrics_rect
        );
    }

    #[test]
    fn headless_home_content_keeps_search_usable_when_constrained() {
        let viewport_size = egui::vec2(420.0, 320.0);
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size);
        let layout = render_home_content_layout(viewport_size);

        assert!(rect_has_area(layout.search_rect));
        assert!(
            rect_is_inside(bounds, layout.search_rect),
            "expected search input to stay usable inside constrained bounds"
        );
    }
}

fn embedder_image_to_egui_image(image: &Image) -> egui::ColorImage {
    let width = image.width as usize;
    let height = image.height as usize;

    match image.format {
        PixelFormat::K8 => egui::ColorImage::from_gray([width, height], image.data()),
        PixelFormat::KA8 => {
            // Convert to rgba
            let data: Vec<u8> = image
                .data()
                .chunks_exact(2)
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                .collect();
            egui::ColorImage::from_rgba_unmultiplied([width, height], &data)
        }
        PixelFormat::RGB8 => egui::ColorImage::from_rgb([width, height], image.data()),
        PixelFormat::RGBA8 => {
            egui::ColorImage::from_rgba_unmultiplied([width, height], image.data())
        }
        PixelFormat::BGRA8 => {
            // Convert from BGRA to RGBA
            let data: Vec<u8> = image
                .data()
                .chunks_exact(4)
                .flat_map(|chunk| [chunk[2], chunk[1], chunk[0], chunk[3]])
                .collect();
            egui::ColorImage::from_rgba_unmultiplied([width, height], &data)
        }
    }
}

fn load_home_favicon_texture(
    ctx: &egui::Context,
    key: &str,
    bytes: &[u8],
) -> Option<(egui::TextureHandle, egui::load::SizedTexture)> {
    let image = home_favicon_color_image(bytes)?;
    let size = egui::vec2(image.size[0] as f32, image.size[1] as f32);
    let handle = ctx.load_texture(format!("home-{key}"), image, egui::TextureOptions::LINEAR);
    let texture = egui::load::SizedTexture::new(handle.id(), size);
    Some((handle, texture))
}

fn home_favicon_color_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    if bytes.is_empty() || bytes.len() > HOME_FAVICON_MAX_BYTES {
        return None;
    }

    let decoded = image::load_from_memory(bytes).ok()?;
    let decoded =
        if decoded.width() > HOME_FAVICON_MAX_SIDE || decoded.height() > HOME_FAVICON_MAX_SIDE {
            decoded.thumbnail(HOME_FAVICON_MAX_SIDE, HOME_FAVICON_MAX_SIDE)
        } else {
            decoded
        };
    let rgba = decoded.to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    if width == 0 || height == 0 {
        return None;
    }

    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        rgba.as_raw(),
    ))
}

fn spawn_home_favicon_fetch(sender: Sender<HomeFaviconFetchResult>, key: String, url: String) {
    let failure_sender = sender.clone();
    let failure_key = key.clone();
    if let Err(error) = thread::Builder::new()
        .name("slate-home-favicon".to_string())
        .spawn(move || {
            let result = fetch_home_favicon(&url);
            let _ = sender.send(HomeFaviconFetchResult { key, result });
        })
    {
        let _ = failure_sender.send(HomeFaviconFetchResult {
            key: failure_key,
            result: Err(error.to_string()),
        });
    }
}

fn fetch_home_favicon(url: &str) -> Result<HomeFaviconBytes, String> {
    let response = BroadwebDaemon::start_default_session()
        .and_then(|daemon| {
            daemon.fetch_http(HttpFetchRequest::default_profile(url).for_subresource())
        })
        .map_err(|error| error.to_string())?;

    if !(200..=299).contains(&response.status_code) {
        return Err(format!("unexpected HTTP status {}", response.status_code));
    }
    if matches!(&response.disposition, FetchDisposition::ErrorPage { .. }) {
        return Err("favicon response was an error page".to_string());
    }
    if response.body.len() > HOME_FAVICON_MAX_BYTES {
        return Err(format!(
            "favicon response exceeded {} bytes",
            HOME_FAVICON_MAX_BYTES
        ));
    }

    Ok(HomeFaviconBytes {
        media_type: response.content_type,
        bytes: response.body,
    })
}

/// Uploads all favicons that have not yet been processed to the GPU.
fn load_pending_favicons(
    ctx: &egui::Context,
    window: &ServoShellWindow,
    texture_cache: &mut HashMap<WebViewId, (egui::TextureHandle, egui::load::SizedTexture)>,
) {
    for id in window.take_pending_favicon_loads() {
        let Some(webview) = window.webview_by_id(id) else {
            continue;
        };
        let Some(favicon) = webview.favicon() else {
            continue;
        };

        let egui_image = embedder_image_to_egui_image(&favicon);
        let handle = ctx.load_texture(format!("favicon-{id:?}"), egui_image, Default::default());
        let texture = egui::load::SizedTexture::new(
            handle.id(),
            egui::vec2(favicon.width as f32, favicon.height as f32),
        );

        // We don't need the handle anymore but we can't drop it either since that would cause
        // the texture to be freed.
        texture_cache.insert(id, (handle, texture));
    }
}
