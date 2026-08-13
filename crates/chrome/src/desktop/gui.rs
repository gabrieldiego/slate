/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::fs;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

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
use url::Url;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use crate::desktop::event_loop::AppEvent;
use crate::desktop::headed_window;
use crate::desktop::protocols::slate::is_slate_home_url;
use crate::desktop::slate_theme::{self, SlateIcon, SlateIconCache, SlateRaster};
use crate::running_app_state::{RunningAppState, UserInterfaceCommand};
use crate::window::ServoShellWindow;

const TAB_STRIP_HEIGHT: f32 = 78.0;
const TAB_STRIP_CONTENT_ALIGN: egui::Align = egui::Align::Max;
const TOOLBAR_HEIGHT: f32 = 88.0;
const APP_RAIL_WIDTH: f32 = 104.0;
const FOOTER_HEIGHT: f32 = 80.0;
const FOOTER_PANEL_MARGIN_X: i8 = 0;
const FOOTER_PANEL_MARGIN_Y: i8 = 8;
const FOOTER_LEFT_PADDING: f32 = 22.0;
const FOOTER_RIGHT_PADDING: f32 = 22.0;
const FOOTER_ITEM_SPACING: f32 = 18.0;
const FOOTER_ICON_SIZE: f32 = 28.0;
const FOOTER_PROTECTION_ICON_LABEL_GAP: f32 = 16.0;
const FOOTER_PROTECTION_LABEL_WIDTH: f32 = 200.0;
const FOOTER_PROTECTION_STATUS_HEIGHT: f32 = 40.0;
const FOOTER_TEXT_SIZE: f32 = 16.0;
const FOOTER_SEPARATOR_HEIGHT: f32 = 28.0;
const FOOTER_SYNC_DOT_SIZE: f32 = 12.0;
const FOOTER_SYNC_DOT_LABEL_GAP: f32 = 10.0;
const FOOTER_SYNC_LABEL_WIDTH: f32 = 64.0;
const FOOTER_SYNC_STATUS_HEIGHT: f32 = 40.0;
const FOOTER_SETTINGS_BUTTON_SIZE: f32 = 40.0;
const FOOTER_SETTINGS_BUTTON_RADIUS: u8 = 8;
const FOOTER_SETTINGS_ICON_SIZE: f32 = 28.0;
const FOOTER_SETTINGS_GEAR_RADIUS: f32 = 9.0;
const FOOTER_SETTINGS_GEAR_CENTER_RADIUS: f32 = 3.5;
const FOOTER_SETTINGS_GEAR_TOOTH_INNER_RADIUS: f32 = 12.0;
const FOOTER_SETTINGS_GEAR_TOOTH_OUTER_RADIUS: f32 = FOOTER_SETTINGS_ICON_SIZE / 2.0;
const FOOTER_SETTINGS_GEAR_STROKE: f32 = 2.0;
const APP_TITLE_WIDTH: f32 = 162.0;
const APP_TITLE_HEIGHT: f32 = TAB_STRIP_HEIGHT;
const APP_TITLE_LEFT_PADDING: f32 = 31.0;
const APP_TITLE_TEXT_SIZE: f32 = 28.0;
const TAB_WIDTH: f32 = 308.0;
const TAB_HEIGHT: f32 = 60.0;
const TAB_CORNER_RADIUS: u8 = 8;
const TAB_INNER_MARGIN_X: i8 = 16;
const TAB_INNER_MARGIN_Y: i8 = 8;
const TAB_CONTENT_HEIGHT: f32 = TAB_HEIGHT - (TAB_INNER_MARGIN_Y as f32 * 2.0);
const TAB_TITLE_MIN_WIDTH: f32 = 80.0;
const TAB_TITLE_TEXT_SIZE: f32 = 20.0;
const TAB_ICON_TITLE_GAP: f32 = 12.0;
const TAB_TITLE_CLOSE_GAP: f32 = 8.0;
const TAB_CLOSE_BUTTON_SIZE: f32 = 28.0;
const TAB_CLOSE_BUTTON_RADIUS: u8 = 6;
const TAB_CLOSE_ICON_SIZE: f32 = 16.0;
const NEW_TAB_LEFT_GAP: f32 = 18.0;
const NEW_TAB_BUTTON_SIZE: f32 = 44.0;
const NEW_TAB_BUTTON_RADIUS: u8 = 8;
const NEW_TAB_ICON_SIZE: f32 = 24.0;
const NEW_TAB_ICON_STROKE: f32 = 2.0;
const TOOLBAR_PANEL_MARGIN_X: i8 = 18;
const TOOLBAR_PANEL_MARGIN_Y: i8 = 10;
const TOOLBAR_ITEM_SPACING: f32 = 20.0;
const TOOLBAR_BUTTON_SIZE: f32 = 40.0;
const TOOLBAR_BUTTON_RADIUS: u8 = 8;
const TOOLBAR_ICON_SIZE: f32 = 24.0;
const TOOLBAR_PRIVACY_ICON_SIZE: f32 = 24.0;
const TOOLBAR_MENU_ICON_WIDTH: f32 = 24.0;
const TOOLBAR_MENU_ICON_GAP: f32 = 7.0;
const TOOLBAR_MENU_ICON_STROKE: f32 = 2.0;
const TOOLBAR_SEPARATOR_HEIGHT: f32 = 36.0;
const RAIL_ICON_SIZE: f32 = 34.0;
const RAIL_BUTTON_SIZE: f32 = 80.0;
const RAIL_BUTTON_RADIUS: u8 = 8;
const RAIL_PANEL_MARGIN_X: i8 = 8;
const RAIL_PANEL_MARGIN_Y: i8 = 0;
const RAIL_TOP_SPACE: f32 = 24.0;
const RAIL_ITEM_GAP: f32 = 16.0;
const TAB_ICON_SIZE: f32 = 24.0;
const ADDRESS_LEADING_GAP: f32 = 28.0;
const ADDRESS_MIN_WIDTH: f32 = 260.0;
const ADDRESS_HEIGHT: f32 = 54.0;
const ADDRESS_TEXT_HEIGHT: f32 = 34.0;
const ADDRESS_INPUT_TEXT_SIZE: f32 = 20.0;
const ADDRESS_CORNER_RADIUS: u8 = 8;
const ADDRESS_INNER_MARGIN_X: i8 = 12;
const ADDRESS_SHADOW_OFFSET: [i8; 2] = [0, 1];
const ADDRESS_SHADOW_BLUR: u8 = 6;
const ADDRESS_SHADOW_SPREAD: u8 = 0;
const ADDRESS_SHADOW_ALPHA: u8 = 10;
const ADDRESS_SECURITY_ICON_SIZE: f32 = 24.0;
const ADDRESS_ICON_GAP: f32 = 8.0;
const ADDRESS_BOOKMARK_ICON_SIZE: f32 = 24.0;
const ADDRESS_BOOKMARK_BUTTON_SIZE: f32 = 28.0;
const ADDRESS_BOOKMARK_BUTTON_RADIUS: u8 = 6;
const ADDRESS_BOOKMARK_RESERVED_WIDTH: f32 = 36.0;
const ADDRESS_TRAILING_CONTROLS_WIDTH: f32 = 176.0;
const HOME_SEARCH_MIN_WIDTH: f32 = 280.0;
const HOME_SEARCH_MAX_WIDTH: f32 = 880.0;
const HOME_SEARCH_HORIZONTAL_PADDING: f32 = 32.0;
const HOME_SEARCH_HEIGHT: f32 = 72.0;
const HOME_SEARCH_FRAME_EXTRA_HEIGHT: f32 = 8.0;
const HOME_SEARCH_TEXT_HEIGHT: f32 = 34.0;
const HOME_SEARCH_INPUT_TEXT_SIZE: f32 = 20.0;
const HOME_SEARCH_INNER_MARGIN_X: i8 = 32;
const HOME_SEARCH_ICON_SIZE: f32 = 24.0;
const HOME_SEARCH_ICON_GAP: f32 = 24.0;
const HOME_SEARCH_CORNER_RADIUS: u8 = 8;
const HOME_TOP_SPACE_FACTOR: f32 = 0.18;
const HOME_TOP_SPACE_MIN: f32 = 48.0;
const HOME_TOP_SPACE_MAX: f32 = 132.0;
const HOME_BOTTOM_MIN_GAP: f32 = 16.0;
const HOME_HERO_SIZE: f32 = 64.0;
const HOME_HERO_TO_SEARCH_GAP: f32 = 44.0;
const HOME_SEARCH_TO_METRICS_GAP: f32 = 62.0;
const HOME_PANEL_SHADOW_OFFSET: [i8; 2] = [0, 2];
const HOME_PANEL_SHADOW_BLUR: u8 = 12;
const HOME_PANEL_SHADOW_SPREAD: u8 = 0;
const HOME_PANEL_SHADOW_ALPHA: u8 = 14;
const HOME_METRIC_CARD_HEIGHT: f32 = 172.0;
const HOME_METRIC_GRID_EXTRA_HEIGHT: f32 = 25.0;
const HOME_METRIC_CARD_MIN_WIDTH: f32 = 156.0;
const HOME_METRIC_CARD_MAX_WIDTH: f32 = 194.0;
const HOME_METRIC_CARD_GAP: f32 = 34.0;
const HOME_METRIC_CARD_INNER_MARGIN_X: i8 = 16;
const HOME_METRIC_CARD_INNER_MARGIN_Y: i8 = 28;
const HOME_METRIC_ICON_SIZE: f32 = 40.0;
const HOME_METRIC_ICON_LABEL_GAP: f32 = 18.0;
const HOME_METRIC_LABEL_TEXT_SIZE: f32 = 17.0;
const HOME_METRIC_DETAIL_TEXT_SIZE: f32 = 14.0;
const HOME_METRIC_DETAIL_GAP: f32 = 4.0;
const HOME_METRIC_BADGE_TEXT_SIZE: f32 = 13.0;
const HOME_METRIC_BADGE_LABEL_GAP: f32 = 8.0;
const HOME_METRIC_BADGE_MARGIN_X: i8 = 8;
const HOME_METRIC_BADGE_MARGIN_Y: i8 = 3;
const HOME_METRIC_BADGE_CORNER_RADIUS: u8 = 10;
const STATUS_BUBBLE_MARGIN_X: f32 = 14.0;
const STATUS_BUBBLE_MARGIN_Y: f32 = 12.0;
const STATUS_BUBBLE_HEIGHT: f32 = 32.0;
const STATUS_BUBBLE_MAX_WIDTH: f32 = 560.0;
const STATUS_BUBBLE_HORIZONTAL_PADDING: f32 = 12.0;
const STATUS_BUBBLE_CORNER_RADIUS: u8 = 8;
const STATUS_TEXT_SIZE: f32 = 13.0;

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

    /// Whether the location has been edited by the user without clicking Go.
    location_dirty: bool,

    /// The [`LoadStatus`] of the active `WebView`.
    load_status: LoadStatus,

    /// The text to display in the status bar on the bottom of the window.
    status_text: Option<String>,

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct HomeMetricsLayout {
    columns: usize,
    card_width: f32,
    spacing: f32,
}

#[derive(Clone, Copy, Debug)]
struct HomeContentLayout {
    hero_rect: egui::Rect,
    search_rect: egui::Rect,
    metrics_rect: egui::Rect,
}

impl Default for HomeContentLayout {
    fn default() -> Self {
        Self {
            hero_rect: egui::Rect::NOTHING,
            search_rect: egui::Rect::NOTHING,
            metrics_rect: egui::Rect::NOTHING,
        }
    }
}

#[derive(Debug)]
struct HomeContentResponse {
    navigation_request: Option<String>,
    layout: HomeContentLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AddressSecurityIcon {
    Slate {
        icon: SlateIcon,
        color: egui::Color32,
    },
    Raster(SlateRaster),
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

fn address_security_icon_for_location(location: &str) -> AddressSecurityIcon {
    match Url::parse(location) {
        Ok(url) if is_slate_home_url(&url) => AddressSecurityIcon::Slate {
            icon: SlateIcon::TopShield,
            color: slate_theme::MUTED,
        },
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

fn address_bookmark_icon_color() -> egui::Color32 {
    slate_theme::MUTED
}

fn address_shadow() -> egui::Shadow {
    egui::Shadow {
        offset: ADDRESS_SHADOW_OFFSET,
        blur: ADDRESS_SHADOW_BLUR,
        spread: ADDRESS_SHADOW_SPREAD,
        color: egui::Color32::from_black_alpha(ADDRESS_SHADOW_ALPHA),
    }
}

fn home_search_width(available_width: f32) -> f32 {
    let available_width = (available_width - HOME_SEARCH_HORIZONTAL_PADDING).max(0.0);
    available_width
        .min(HOME_SEARCH_MAX_WIDTH)
        .max(HOME_SEARCH_MIN_WIDTH.min(available_width))
}

fn home_search_content_width(search_width: f32) -> f32 {
    (search_width - f32::from(HOME_SEARCH_INNER_MARGIN_X) * 2.0).max(0.0)
}

fn home_metric_card_content_width(card_width: f32) -> f32 {
    (card_width - f32::from(HOME_METRIC_CARD_INNER_MARGIN_X) * 2.0).max(0.0)
}

fn home_metric_card_content_height() -> f32 {
    (HOME_METRIC_CARD_HEIGHT - f32::from(HOME_METRIC_CARD_INNER_MARGIN_Y) * 2.0).max(0.0)
}

fn home_metric_badge_width(text: &str) -> f32 {
    text.chars().count() as f32 * HOME_METRIC_BADGE_TEXT_SIZE * 0.58
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
        + HOME_HERO_TO_SEARCH_GAP
        + home_search_rendered_height()
        + HOME_SEARCH_TO_METRICS_GAP
        + home_metrics_rendered_height()
        + HOME_BOTTOM_MIN_GAP
}

#[cfg(test)]
fn home_metrics_row_width(layout: HomeMetricsLayout) -> f32 {
    layout.card_width * layout.columns as f32
        + layout.spacing * (layout.columns.saturating_sub(1) as f32)
}

#[cfg(test)]
fn home_content_stack_height(available_height: f32) -> f32 {
    home_top_space(available_height)
        + HOME_HERO_SIZE
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
        1672.0 - APP_RAIL_WIDTH,
        940.0 - TAB_STRIP_HEIGHT - TOOLBAR_HEIGHT - FOOTER_HEIGHT,
    )
}

fn tab_content_width() -> f32 {
    (TAB_WIDTH - f32::from(TAB_INNER_MARGIN_X) * 2.0).max(0.0)
}

fn tab_title_width(available_width: f32) -> f32 {
    (available_width
        - TAB_ICON_SIZE
        - TAB_ICON_TITLE_GAP
        - TAB_CLOSE_BUTTON_SIZE
        - TAB_TITLE_CLOSE_GAP)
        .max(TAB_TITLE_MIN_WIDTH)
}

fn tab_title_color(active: bool) -> egui::Color32 {
    if active {
        slate_theme::TEXT
    } else {
        slate_theme::MUTED
    }
}

fn tab_close_icon_color(active: bool) -> egui::Color32 {
    if active {
        slate_theme::TEXT
    } else {
        slate_theme::MUTED
    }
}

fn toolbar_navigation_icon_color(enabled: bool) -> egui::Color32 {
    if enabled {
        slate_theme::TEXT
    } else {
        slate_theme::MUTED
    }
}

fn footer_sync_status_width() -> f32 {
    FOOTER_SYNC_DOT_SIZE + FOOTER_SYNC_DOT_LABEL_GAP + FOOTER_SYNC_LABEL_WIDTH
}

fn footer_sync_dot_radius() -> f32 {
    FOOTER_SYNC_DOT_SIZE / 2.0
}

fn footer_settings_icon_color() -> egui::Color32 {
    slate_theme::MUTED
}

fn footer_protection_status_width() -> f32 {
    FOOTER_ICON_SIZE + FOOTER_PROTECTION_ICON_LABEL_GAP + FOOTER_PROTECTION_LABEL_WIDTH
}

fn tab_corner_radius() -> egui::CornerRadius {
    egui::CornerRadius {
        nw: TAB_CORNER_RADIUS,
        ne: TAB_CORNER_RADIUS,
        sw: 0,
        se: 0,
    }
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

        Self {
            rendering_context,
            context,
            toolbar_height: Default::default(),
            webview_origin: Point2D::zero(),
            webview_size: Size2D::zero(),
            webview_contains_native_chrome: false,
            location: initial_url.to_string(),
            home_search: String::new(),
            location_dirty: false,
            load_status: LoadStatus::Complete,
            status_text: None,
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

    /// Return true iff the given position is over the egui toolbar.
    pub(crate) fn is_in_egui_toolbar_rect(
        &self,
        position: Point2D<f32, DeviceIndependentPixel>,
    ) -> bool {
        egui_chrome_owns_position(
            self.webview_origin,
            self.webview_size,
            self.webview_contains_native_chrome,
            position,
        )
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
            let stroke = egui::Stroke::new(NEW_TAB_ICON_STROKE, slate_theme::TEXT);
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

    fn footer_settings_button(ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(FOOTER_SETTINGS_BUTTON_SIZE),
            egui::Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            if response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    FOOTER_SETTINGS_BUTTON_RADIUS,
                    slate_theme::PANEL_HOVER,
                );
            }

            let center = rect.center();
            let stroke =
                egui::Stroke::new(FOOTER_SETTINGS_GEAR_STROKE, footer_settings_icon_color());
            ui.painter()
                .circle_stroke(center, FOOTER_SETTINGS_GEAR_RADIUS, stroke);
            ui.painter()
                .circle_stroke(center, FOOTER_SETTINGS_GEAR_CENTER_RADIUS, stroke);

            for tooth_index in 0..8 {
                let angle = std::f32::consts::TAU * tooth_index as f32 / 8.0;
                let direction_x = angle.cos();
                let direction_y = angle.sin();
                ui.painter().line_segment(
                    [
                        egui::pos2(
                            center.x + direction_x * FOOTER_SETTINGS_GEAR_TOOTH_INNER_RADIUS,
                            center.y + direction_y * FOOTER_SETTINGS_GEAR_TOOTH_INNER_RADIUS,
                        ),
                        egui::pos2(
                            center.x + direction_x * FOOTER_SETTINGS_GEAR_TOOTH_OUTER_RADIUS,
                            center.y + direction_y * FOOTER_SETTINGS_GEAR_TOOTH_OUTER_RADIUS,
                        ),
                    ],
                    stroke,
                );
            }
        }

        response
    }

    fn icon_image(texture: egui::load::SizedTexture, size: f32) -> egui::Image<'static> {
        egui::Image::from_texture(texture)
            .fit_to_exact_size(egui::vec2(size, size))
            .bg_fill(egui::Color32::TRANSPARENT)
    }

    fn vertical_separator(ui: &mut egui::Ui, height: f32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().line_segment(
                [
                    egui::pos2(rect.center().x, rect.top()),
                    egui::pos2(rect.center().x, rect.bottom()),
                ],
                egui::Stroke::new(1.0, slate_theme::BORDER),
            );
        }
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

            let center = rect.center();
            for offset in [-TOOLBAR_MENU_ICON_GAP, 0.0, TOOLBAR_MENU_ICON_GAP] {
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - TOOLBAR_MENU_ICON_WIDTH / 2.0, center.y + offset),
                        egui::pos2(center.x + TOOLBAR_MENU_ICON_WIDTH / 2.0, center.y + offset),
                    ],
                    egui::Stroke::new(TOOLBAR_MENU_ICON_STROKE, slate_theme::TEXT),
                );
            }
        }
        response
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

    fn tab_title_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(tab_title_width(tab_content_width()), TAB_CONTENT_HEIGHT),
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

    fn active_webview_is_home(window: &ServoShellWindow) -> bool {
        window
            .active_webview()
            .and_then(|webview| webview.url())
            .as_ref()
            .is_some_and(is_slate_home_url)
    }

    fn rail_icon_button(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        icon: SlateIcon,
        selected: bool,
        tooltip: &str,
    ) {
        let texture = slate_icons.texture(
            ui.ctx(),
            icon,
            if selected {
                slate_theme::TEAL
            } else {
                slate_theme::TEXT
            },
        );

        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(RAIL_BUTTON_SIZE), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let fill = if selected {
                slate_theme::TEAL_SOFT
            } else if response.hovered() {
                slate_theme::PANEL_HOVER
            } else {
                egui::Color32::TRANSPARENT
            };
            if fill != egui::Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, RAIL_BUTTON_RADIUS, fill);
            }
            let icon_rect =
                egui::Rect::from_center_size(rect.center(), Vec2::splat(RAIL_ICON_SIZE));
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        response.widget_info(|| {
            let mut info = WidgetInfo::new(WidgetType::Button);
            info.label = Some(tooltip.into());
            info.selected = Some(selected);
            info
        });
        response.on_hover_text(tooltip);
    }

    fn draw_app_rail(ui: &mut egui::Ui, slate_icons: &mut SlateIconCache) {
        ui.vertical_centered(|ui| {
            ui.add_space(RAIL_TOP_SPACE);
            Self::rail_icon_button(ui, slate_icons, SlateIcon::AppWeb, true, "Web");
            ui.add_space(RAIL_ITEM_GAP);
            Self::rail_icon_button(ui, slate_icons, SlateIcon::AppDownloads, false, "Downloads");
            ui.add_space(RAIL_ITEM_GAP);
            Self::rail_icon_button(ui, slate_icons, SlateIcon::AppCalendar, false, "Calendar");
            ui.add_space(RAIL_ITEM_GAP);
            Self::rail_icon_button(ui, slate_icons, SlateIcon::AppMessaging, false, "Messages");
        });
    }

    fn draw_app_title(ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(APP_TITLE_WIDTH, APP_TITLE_HEIGHT),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 0.0, slate_theme::SURFACE);
        ui.painter().line_segment(
            [rect.right_top(), rect.right_bottom()],
            egui::Stroke::new(1.0, slate_theme::BORDER),
        );
        ui.painter().text(
            egui::pos2(rect.min.x + APP_TITLE_LEFT_PADDING, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Slate",
            egui::FontId::proportional(APP_TITLE_TEXT_SIZE),
            slate_theme::TEXT,
        );
    }

    fn draw_footer_sync_status(ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(footer_sync_status_width(), FOOTER_SYNC_STATUS_HEIGHT),
            egui::Sense::hover(),
        );

        if ui.is_rect_visible(rect) {
            let dot_center = egui::pos2(rect.left() + FOOTER_SYNC_DOT_SIZE / 2.0, rect.center().y);
            ui.painter()
                .circle_filled(dot_center, footer_sync_dot_radius(), slate_theme::TEAL);
            ui.painter().text(
                egui::pos2(
                    dot_center.x + FOOTER_SYNC_DOT_SIZE / 2.0 + FOOTER_SYNC_DOT_LABEL_GAP,
                    rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                "Sync On",
                egui::FontId::proportional(FOOTER_TEXT_SIZE),
                slate_theme::TEXT,
            );
        }

        response.on_hover_text("Sync On");
    }

    fn draw_footer_protection_status(ui: &mut egui::Ui, texture: egui::load::SizedTexture) {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(
                footer_protection_status_width(),
                FOOTER_PROTECTION_STATUS_HEIGHT,
            ),
            egui::Sense::hover(),
        );

        if ui.is_rect_visible(rect) {
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(rect.left() + FOOTER_ICON_SIZE / 2.0, rect.center().y),
                Vec2::splat(FOOTER_ICON_SIZE),
            );
            ui.painter().image(
                texture.id,
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            ui.painter().text(
                egui::pos2(
                    icon_rect.right() + FOOTER_PROTECTION_ICON_LABEL_GAP,
                    rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                "Protected. Private. Yours.",
                egui::FontId::proportional(FOOTER_TEXT_SIZE),
                slate_theme::TEXT,
            );
        }

        let enabled = ui.is_enabled();
        response.widget_info(move || {
            WidgetInfo::labeled(WidgetType::Label, enabled, "Protected. Private. Yours.")
        });
        response.on_hover_text("Protected. Private. Yours.");
    }

    fn draw_footer(ui: &mut egui::Ui, slate_icons: &mut SlateIconCache) {
        ui.spacing_mut().item_spacing = egui::vec2(FOOTER_ITEM_SPACING, 0.0);
        ui.horizontal_centered(|ui| {
            ui.add_space(FOOTER_LEFT_PADDING);
            let footer_icon =
                slate_icons.texture(ui.ctx(), SlateIcon::HomeFooterShield, slate_theme::TEAL);
            Self::draw_footer_protection_status(ui, footer_icon);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(FOOTER_RIGHT_PADDING);
                let settings_button = Gui::footer_settings_button(ui);
                settings_button.widget_info(|| {
                    let mut info = WidgetInfo::new(WidgetType::Button);
                    info.label = Some("Settings".into());
                    info
                });
                settings_button.on_hover_text("Settings");
                Self::vertical_separator(ui, FOOTER_SEPARATOR_HEIGHT);
                Self::draw_footer_sync_status(ui);
            });
        });
    }

    fn draw_badge(ui: &mut egui::Ui, text: &str, fill: egui::Color32) {
        let width = home_metric_badge_width(text);
        let height = HOME_METRIC_BADGE_TEXT_SIZE + f32::from(HOME_METRIC_BADGE_MARGIN_Y) * 2.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, HOME_METRIC_BADGE_CORNER_RADIUS, fill);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(HOME_METRIC_BADGE_TEXT_SIZE),
                slate_theme::SURFACE,
            );
        }

        let enabled = ui.is_enabled();
        response.widget_info(move || WidgetInfo::labeled(WidgetType::Label, enabled, text));
    }

    fn draw_home_metric_card(
        ui: &mut egui::Ui,
        slate_icons: &mut SlateIconCache,
        width: f32,
        icon: SlateIcon,
        label: &str,
        badge: Option<(&str, egui::Color32)>,
        detail: Option<&str>,
    ) {
        egui::Frame::NONE
            .fill(slate_theme::SURFACE)
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
                ui.allocate_ui_with_layout(
                    content_size,
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        let texture = slate_icons.texture(ui.ctx(), icon, slate_theme::TEAL);
                        ui.add(Self::icon_image(texture, HOME_METRIC_ICON_SIZE));
                        ui.add_space(HOME_METRIC_ICON_LABEL_GAP);
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = HOME_METRIC_BADGE_LABEL_GAP;
                            ui.label(
                                egui::RichText::new(label)
                                    .size(HOME_METRIC_LABEL_TEXT_SIZE)
                                    .color(slate_theme::TEXT),
                            );
                            if let Some((text, fill)) = badge {
                                Self::draw_badge(ui, text, fill);
                            }
                        });
                        if let Some(detail) = detail {
                            ui.add_space(HOME_METRIC_DETAIL_GAP);
                            ui.label(
                                egui::RichText::new(detail)
                                    .size(HOME_METRIC_DETAIL_TEXT_SIZE)
                                    .color(slate_theme::MUTED),
                            );
                        }
                    },
                );
            });
    }

    fn draw_home_metrics(ui: &mut egui::Ui, slate_icons: &mut SlateIconCache) -> egui::Rect {
        let layout = home_metrics_layout(ui.available_width());

        egui::Grid::new("slate_home_metrics")
            .num_columns(layout.columns)
            .spacing(egui::vec2(layout.spacing, layout.spacing))
            .show(ui, |ui| {
                for (index, (icon, label, badge, detail)) in [
                    (SlateIcon::HomeMetricPrivacy, "Privacy First", None, None),
                    (
                        SlateIcon::HomeMetricLock,
                        "Tracker Blocked",
                        Some(("23", slate_theme::AMBER)),
                        None,
                    ),
                    (
                        SlateIcon::HomeMetricAds,
                        "Ads Blocked",
                        Some(("184", slate_theme::BLUE)),
                        None,
                    ),
                    (
                        SlateIcon::HomeMetricTime,
                        "Time Saved",
                        None,
                        Some("2h 14m"),
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    Self::draw_home_metric_card(
                        ui,
                        slate_icons,
                        layout.card_width,
                        icon,
                        label,
                        badge,
                        detail,
                    );
                    if (index + 1) % layout.columns == 0 {
                        ui.end_row();
                    }
                }
            })
            .response
            .rect
    }

    fn draw_home_content(
        ui: &mut egui::Ui,
        home_rect: egui::Rect,
        slate_icons: &mut SlateIconCache,
        home_search: &mut String,
    ) -> HomeContentResponse {
        let mut layout = HomeContentLayout::default();
        let mut navigation_request = None;

        ui.add_space(home_top_space(home_rect.height()));
        ui.vertical_centered(|ui| {
            let hero = slate_icons.texture(ui.ctx(), SlateIcon::HomeHeroShield, slate_theme::TEAL);
            let hero_response = ui.add(
                egui::Image::from_texture(hero)
                    .fit_to_exact_size(egui::vec2(HOME_HERO_SIZE, HOME_HERO_SIZE)),
            );
            layout.hero_rect = hero_response.rect;
            ui.add_space(HOME_HERO_TO_SEARCH_GAP);

            let search_width = home_search_width(ui.available_width());
            let search_content_width = home_search_content_width(search_width);
            let home_search_id = egui::Id::new("home_search_input");
            let search_frame_response = egui::Frame::NONE
                .fill(slate_theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, slate_theme::BORDER))
                .corner_radius(HOME_SEARCH_CORNER_RADIUS)
                .shadow(home_panel_shadow())
                .inner_margin(egui::Margin::symmetric(HOME_SEARCH_INNER_MARGIN_X, 0))
                .show(ui, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(search_content_width, HOME_SEARCH_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let search_icon = slate_icons.raster_mask_texture(
                                ui.ctx(),
                                SlateRaster::Search,
                                slate_theme::MUTED,
                            );
                            ui.add(Self::icon_image(search_icon, HOME_SEARCH_ICON_SIZE));
                            ui.add_space(HOME_SEARCH_ICON_GAP);
                            ui.add_sized(
                                [ui.available_width(), HOME_SEARCH_TEXT_HEIGHT],
                                egui::TextEdit::singleline(home_search)
                                    .id(home_search_id)
                                    .font(egui::FontId::proportional(HOME_SEARCH_INPUT_TEXT_SIZE))
                                    .frame(egui::Frame::NONE)
                                    .hint_text("Search the web or enter an address"),
                            )
                        },
                    )
                    .inner
                });
            let search_response = search_frame_response.inner;
            layout.search_rect = search_frame_response.response.rect;

            if search_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let request = home_search.trim().to_owned();
                if !request.is_empty() {
                    navigation_request = Some(request);
                    home_search.clear();
                }
            }

            ui.add_space(HOME_SEARCH_TO_METRICS_GAP);
            let metrics_height = ui.available_height().max(0.0);
            let metrics_response = ui.allocate_ui_with_layout(
                egui::vec2(search_width, metrics_height),
                egui::Layout::top_down(egui::Align::Center),
                |ui| Self::draw_home_metrics(ui, slate_icons),
            );
            layout.metrics_rect = metrics_response.inner;
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
        home_search: &mut String,
        window: &ServoShellWindow,
    ) {
        egui::Area::new(Id::new("slate_home_view"))
            .order(Order::Foreground)
            .fixed_pos(available_rect.min)
            .show(ctx, |ui| {
                let home_rect = egui::Rect::from_min_size(ui.min_rect().min, available_rect.size());
                ui.set_min_size(home_rect.size());
                ui.painter().rect_filled(home_rect, 0.0, slate_theme::BG);
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(home_rect), |ui| {
                        Self::draw_home_content(ui, home_rect, slate_icons, home_search)
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
            egui::Color32::from_black_alpha(12),
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
    ) {
        let label = match (webview.page_title(), webview.url()) {
            (_, Some(url)) if is_slate_home_url(&url) => "New Tab".into(),
            (Some(title), _) if !title.is_empty() => title,
            (_, Some(url)) => url.to_string(),
            _ => "New Tab".into(),
        };

        let inactive_bg_color = slate_theme::PANEL;
        let inactive_hover_bg_color = slate_theme::PANEL_HOVER;
        let active_bg_color = slate_theme::SURFACE;
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

        // Setup a tab frame that will contain the favicon, title and close button
        let mut tab_frame = egui::Frame::NONE
            .fill(tab_content_bg_color)
            .stroke(egui::Stroke::new(1.0, slate_theme::BORDER))
            .corner_radius(tab_corner_radius())
            .inner_margin(egui::Margin::symmetric(
                TAB_INNER_MARGIN_X,
                TAB_INNER_MARGIN_Y,
            ))
            .begin(ui);
        {
            tab_frame.content_ui.set_width(tab_content_width());
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
            tab_frame
                .content_ui
                .add(Self::icon_image(icon, TAB_ICON_SIZE));
            tab_frame.content_ui.add_space(TAB_ICON_TITLE_GAP);

            let tab = Self::tab_title_button(&mut tab_frame.content_ui, &label, active);
            tab_frame.content_ui.add_space(TAB_TITLE_CLOSE_GAP);

            let close_icon = if active {
                active_close_icon
            } else {
                inactive_close_icon
            };
            let close_button = Self::tab_close_button(&mut tab_frame.content_ui, close_icon);
            if close_button.clicked() || close_button.middle_clicked() || tab.middle_clicked() {
                window
                    .queue_user_interface_command(UserInterfaceCommand::CloseWebView(webview.id()));
            } else if !active && tab.clicked() {
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
        let Self {
            rendering_context,
            context,
            toolbar_height,
            webview_origin,
            webview_size,
            webview_contains_native_chrome,
            location,
            home_search,
            location_dirty,
            favicon_textures,
            slate_icons,
            ..
        } = self;

        let winit_window = headed_window.winit_window();
        context.run(winit_window, |ctx| {
            slate_theme::apply(ctx);
            load_pending_favicons(ctx, window, favicon_textures);
            let active_webview_is_home = Self::active_webview_is_home(window);
            *webview_contains_native_chrome = active_webview_is_home;

            // TODO: While in fullscreen add some way to mitigate the increased phishing risk
            // when not displaying the URL bar: https://github.com/servo/servo/issues/32443
            if winit_window.fullscreen().is_none() {
                let tabs_frame = egui::Frame::NONE
                    .fill(slate_theme::BG)
                    .inner_margin(egui::Margin::symmetric(0, 0));
                Panel::top("tabs")
                    .exact_size(TAB_STRIP_HEIGHT)
                    .frame(tabs_frame)
                    .show_separator_line(true)
                    .show_inside(ctx, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        ui.allocate_ui_with_layout(
                            ui.available_size(),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                Self::draw_app_title(ui);

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
                                                for (index, (id, webview)) in
                                                    window.webviews().into_iter().enumerate()
                                                {
                                                    let favicon = favicon_textures
                                                        .get(&id)
                                                        .map(|(_, favicon)| favicon)
                                                        .copied();
                                                    let fallback_icon_color = if window
                                                        .active_webview()
                                                        .map(|webview| webview.id())
                                                        == Some(id)
                                                    {
                                                        slate_theme::TEXT
                                                    } else {
                                                        slate_theme::MUTED
                                                    };
                                                    let fallback_icon = slate_icons.texture(
                                                        ui.ctx(),
                                                        Self::fallback_tab_icon(index),
                                                        fallback_icon_color,
                                                    );
                                                    let close_icon = slate_icons
                                                        .raster_mask_texture(
                                                            ui.ctx(),
                                                            SlateRaster::TabClose,
                                                            tab_close_icon_color(true),
                                                        );
                                                    let inactive_close_icon = slate_icons
                                                        .raster_mask_texture(
                                                            ui.ctx(),
                                                            SlateRaster::TabCloseMuted,
                                                            tab_close_icon_color(false),
                                                        );
                                                    Self::browser_tab(
                                                        ui,
                                                        window,
                                                        webview,
                                                        favicon,
                                                        fallback_icon,
                                                        close_icon,
                                                        inactive_close_icon,
                                                    );
                                                }

                                                ui.add_space(NEW_TAB_LEFT_GAP);
                                                let new_tab_button = Gui::new_tab_button(ui);
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
                    });

                let rail_frame = egui::Frame::NONE.fill(slate_theme::SURFACE).inner_margin(
                    egui::Margin::symmetric(RAIL_PANEL_MARGIN_X, RAIL_PANEL_MARGIN_Y),
                );
                Panel::left("app_rail")
                    .exact_size(APP_RAIL_WIDTH)
                    .frame(rail_frame)
                    .show_separator_line(true)
                    .show_inside(ctx, |ui| Self::draw_app_rail(ui, slate_icons));

                let toolbar_frame = egui::Frame::NONE.fill(slate_theme::SURFACE).inner_margin(
                    egui::Margin::symmetric(TOOLBAR_PANEL_MARGIN_X, TOOLBAR_PANEL_MARGIN_Y),
                );
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
                                let back_button = Gui::toolbar_hover_raster_button(
                                    ui,
                                    slate_icons,
                                    if self.can_go_back {
                                        SlateRaster::NavBack
                                    } else {
                                        SlateRaster::NavBackDisabled
                                    },
                                    SlateRaster::NavBackHover,
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

                                let forward_button = Gui::toolbar_hover_raster_button(
                                    ui,
                                    slate_icons,
                                    if self.can_go_forward {
                                        SlateRaster::NavForward
                                    } else {
                                        SlateRaster::NavForwardDisabled
                                    },
                                    SlateRaster::NavForwardHover,
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

                                match self.load_status {
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
                                        let reload_button = Gui::toolbar_hover_raster_button(
                                            ui,
                                            slate_icons,
                                            SlateRaster::NavReload,
                                            SlateRaster::NavReloadHover,
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
                                    .fill(slate_theme::SURFACE)
                                    .stroke(egui::Stroke::new(1.0, slate_theme::BORDER))
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
                                            let page_info_icon =
                                                match address_security_icon_for_location(location) {
                                                    AddressSecurityIcon::Slate { icon, color } => {
                                                        slate_icons.texture(ui.ctx(), icon, color)
                                                    }
                                                    AddressSecurityIcon::Raster(raster) => {
                                                        slate_icons.raster_texture(ui.ctx(), raster)
                                                    }
                                                };
                                            ui.add(Self::icon_image(
                                                page_info_icon,
                                                ADDRESS_SECURITY_ICON_SIZE,
                                            ));
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
                                            bookmark_button.widget_info(|| {
                                                let mut info = WidgetInfo::new(WidgetType::Button);
                                                info.label = Some("Bookmark".into());
                                                info
                                            });
                                            bookmark_button.on_hover_text("Bookmark");
                                            text_response
                                        })
                                        .inner
                                    })
                                    .inner;

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
                                Self::vertical_separator(ui, TOOLBAR_SEPARATOR_HEIGHT);

                                let mut experimental_preferences_enabled =
                                    state.experimental_preferences_enabled();
                                let prefs_toggle =
                                    Gui::toolbar_menu_button(ui, experimental_preferences_enabled)
                                        .on_hover_text("Enable experimental prefs");
                                prefs_toggle.widget_info(|| {
                                    let mut info = WidgetInfo::new(WidgetType::Button);
                                    info.label = Some("Enable experimental preferences".into());
                                    info.selected = Some(experimental_preferences_enabled);
                                    info
                                });
                                if prefs_toggle.clicked() {
                                    experimental_preferences_enabled =
                                        !experimental_preferences_enabled;
                                    state.set_experimental_preferences_enabled(
                                        experimental_preferences_enabled,
                                    );
                                    *location_dirty = false;
                                    window.queue_user_interface_command(
                                        UserInterfaceCommand::ReloadAll,
                                    );
                                }
                            },
                        );
                    });

                let footer_frame = egui::Frame::NONE.fill(slate_theme::SURFACE).inner_margin(
                    egui::Margin::symmetric(FOOTER_PANEL_MARGIN_X, FOOTER_PANEL_MARGIN_Y),
                );
                Panel::bottom("footer")
                    .exact_size(FOOTER_HEIGHT)
                    .frame(footer_frame)
                    .show_separator_line(true)
                    .show_inside(ctx, |ui| Self::draw_footer(ui, slate_icons));
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

            if active_webview_is_home {
                Self::draw_home_view(ctx, available_rect, slate_icons, home_search, window);
            }

            if let Some(status_text) = &self.status_text {
                Self::draw_status_text(ctx, available_rect, status_text);
            }

            if !active_webview_is_home {
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
            .and_then(|webview| Some(webview.url()?.to_string()));
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

    fn update_status_text(&mut self, window: &ServoShellWindow) -> bool {
        let state_status = window
            .active_webview()
            .and_then(|webview| webview.status_text());
        let old_status = std::mem::replace(&mut self.status_text, state_status);
        old_status != self.status_text
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

    pub(crate) fn set_zoom_factor(&self, factor: f32) {
        self.context.egui_ctx.set_zoom_factor(factor);
    }

    pub(crate) fn notify_accessibility_tree_update(&mut self, tree_update: accesskit::TreeUpdate) {
        self.pending_accesskit_updates.push(tree_update);
    }
}

#[cfg(test)]
mod tests {
    use euclid::{Point2D, Size2D};
    use servo::DeviceIndependentPixel;

    use super::{
        ADDRESS_BOOKMARK_BUTTON_RADIUS, ADDRESS_BOOKMARK_BUTTON_SIZE, ADDRESS_BOOKMARK_ICON_SIZE,
        ADDRESS_BOOKMARK_RESERVED_WIDTH, ADDRESS_CORNER_RADIUS, ADDRESS_ICON_GAP,
        ADDRESS_INNER_MARGIN_X, ADDRESS_LEADING_GAP, ADDRESS_MIN_WIDTH, ADDRESS_SECURITY_ICON_SIZE,
        ADDRESS_SHADOW_ALPHA, ADDRESS_SHADOW_BLUR, ADDRESS_SHADOW_OFFSET, ADDRESS_SHADOW_SPREAD,
        ADDRESS_TRAILING_CONTROLS_WIDTH, AddressSecurityIcon, Gui, HOME_METRIC_CARD_MIN_WIDTH,
        HomeContentLayout, SlateIconCache, address_bookmark_icon_color,
        address_security_icon_for_location, concept_screenshot_home_view_size,
        default_opening_home_view_height, default_opening_home_view_size,
        footer_protection_status_width, footer_settings_icon_color, footer_sync_dot_radius,
        footer_sync_status_width, home_content_stack_height, home_metric_badge_width,
        home_metric_card_content_height, home_metric_card_content_width, home_metrics_layout,
        home_metrics_rendered_height, home_metrics_row_width, home_search_rendered_height,
        home_search_width, home_top_space, slate_theme, status_bubble_label, status_bubble_width,
        tab_close_icon_color, tab_content_width, tab_corner_radius, tab_title_color,
        tab_title_width, toolbar_address_width, toolbar_navigation_icon_color,
    };
    use super::{
        ADDRESS_HEIGHT, ADDRESS_INPUT_TEXT_SIZE, APP_RAIL_WIDTH, APP_TITLE_HEIGHT,
        APP_TITLE_LEFT_PADDING, APP_TITLE_TEXT_SIZE, FOOTER_HEIGHT, FOOTER_ICON_SIZE,
        FOOTER_ITEM_SPACING, FOOTER_LEFT_PADDING, FOOTER_PANEL_MARGIN_X, FOOTER_PANEL_MARGIN_Y,
        FOOTER_PROTECTION_ICON_LABEL_GAP, FOOTER_PROTECTION_LABEL_WIDTH,
        FOOTER_PROTECTION_STATUS_HEIGHT, FOOTER_RIGHT_PADDING, FOOTER_SEPARATOR_HEIGHT,
        FOOTER_SETTINGS_BUTTON_RADIUS, FOOTER_SETTINGS_BUTTON_SIZE,
        FOOTER_SETTINGS_GEAR_CENTER_RADIUS, FOOTER_SETTINGS_GEAR_RADIUS,
        FOOTER_SETTINGS_GEAR_STROKE, FOOTER_SETTINGS_GEAR_TOOTH_INNER_RADIUS,
        FOOTER_SETTINGS_GEAR_TOOTH_OUTER_RADIUS, FOOTER_SETTINGS_ICON_SIZE,
        FOOTER_SYNC_DOT_LABEL_GAP, FOOTER_SYNC_DOT_SIZE, FOOTER_SYNC_LABEL_WIDTH,
        FOOTER_SYNC_STATUS_HEIGHT, FOOTER_TEXT_SIZE, HOME_BOTTOM_MIN_GAP, HOME_HERO_SIZE,
        HOME_HERO_TO_SEARCH_GAP, HOME_METRIC_BADGE_CORNER_RADIUS, HOME_METRIC_BADGE_LABEL_GAP,
        HOME_METRIC_BADGE_MARGIN_X, HOME_METRIC_BADGE_MARGIN_Y, HOME_METRIC_BADGE_TEXT_SIZE,
        HOME_METRIC_CARD_GAP, HOME_METRIC_CARD_HEIGHT, HOME_METRIC_CARD_INNER_MARGIN_X,
        HOME_METRIC_CARD_INNER_MARGIN_Y, HOME_METRIC_CARD_MAX_WIDTH, HOME_METRIC_DETAIL_GAP,
        HOME_METRIC_DETAIL_TEXT_SIZE, HOME_METRIC_GRID_EXTRA_HEIGHT, HOME_METRIC_ICON_LABEL_GAP,
        HOME_METRIC_ICON_SIZE, HOME_METRIC_LABEL_TEXT_SIZE, HOME_PANEL_SHADOW_ALPHA,
        HOME_PANEL_SHADOW_BLUR, HOME_PANEL_SHADOW_OFFSET, HOME_PANEL_SHADOW_SPREAD,
        HOME_SEARCH_FRAME_EXTRA_HEIGHT, HOME_SEARCH_ICON_SIZE, HOME_SEARCH_INPUT_TEXT_SIZE,
        HOME_SEARCH_TO_METRICS_GAP, HOME_TOP_SPACE_FACTOR, HOME_TOP_SPACE_MAX, HOME_TOP_SPACE_MIN,
        NEW_TAB_BUTTON_RADIUS, NEW_TAB_BUTTON_SIZE, NEW_TAB_ICON_SIZE, NEW_TAB_ICON_STROKE,
        NEW_TAB_LEFT_GAP, STATUS_BUBBLE_CORNER_RADIUS, STATUS_BUBBLE_HEIGHT,
        STATUS_BUBBLE_HORIZONTAL_PADDING, STATUS_BUBBLE_MARGIN_X, STATUS_BUBBLE_MARGIN_Y,
        STATUS_BUBBLE_MAX_WIDTH, STATUS_TEXT_SIZE, TAB_CLOSE_BUTTON_RADIUS, TAB_CLOSE_ICON_SIZE,
        TAB_CONTENT_HEIGHT, TAB_CORNER_RADIUS, TAB_HEIGHT, TAB_ICON_SIZE, TAB_ICON_TITLE_GAP,
        TAB_INNER_MARGIN_X, TAB_INNER_MARGIN_Y, TAB_STRIP_CONTENT_ALIGN, TAB_STRIP_HEIGHT,
        TAB_TITLE_CLOSE_GAP, TAB_TITLE_MIN_WIDTH, TAB_TITLE_TEXT_SIZE, TAB_WIDTH,
        TOOLBAR_BUTTON_RADIUS, TOOLBAR_BUTTON_SIZE, TOOLBAR_HEIGHT, TOOLBAR_ICON_SIZE,
        TOOLBAR_ITEM_SPACING, TOOLBAR_MENU_ICON_GAP, TOOLBAR_MENU_ICON_STROKE,
        TOOLBAR_MENU_ICON_WIDTH, TOOLBAR_PANEL_MARGIN_X, TOOLBAR_PANEL_MARGIN_Y,
        TOOLBAR_PRIVACY_ICON_SIZE, TOOLBAR_SEPARATOR_HEIGHT, egui_chrome_owns_position,
    };
    use super::{
        HOME_SEARCH_CORNER_RADIUS, HOME_SEARCH_HEIGHT, HOME_SEARCH_HORIZONTAL_PADDING,
        HOME_SEARCH_ICON_GAP, HOME_SEARCH_INNER_MARGIN_X, HOME_SEARCH_MAX_WIDTH,
        HOME_SEARCH_MIN_WIDTH, HOME_SEARCH_TEXT_HEIGHT,
    };
    use super::{
        RAIL_BUTTON_RADIUS, RAIL_BUTTON_SIZE, RAIL_ICON_SIZE, RAIL_ITEM_GAP, RAIL_PANEL_MARGIN_X,
        RAIL_PANEL_MARGIN_Y, RAIL_TOP_SPACE, TAB_CLOSE_BUTTON_SIZE,
    };

    const LAYOUT_EPSILON: f32 = 1.0;

    fn chrome_webview_origin() -> Point2D<f32, DeviceIndependentPixel> {
        Point2D::new(APP_RAIL_WIDTH, TAB_STRIP_HEIGHT + TOOLBAR_HEIGHT)
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
            let response =
                Gui::draw_home_content(ui, screen_rect, &mut slate_icons, &mut home_search);
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
    fn static_chrome_dimensions_match_concept_offsets() {
        assert_eq!(APP_RAIL_WIDTH, 104.0);
        assert_eq!(RAIL_ICON_SIZE, 34.0);
        assert_eq!(RAIL_BUTTON_SIZE, 80.0);
        assert_eq!(RAIL_BUTTON_RADIUS, 8);
        assert_eq!(RAIL_PANEL_MARGIN_X, 8);
        assert_eq!(RAIL_PANEL_MARGIN_Y, 0);
        assert_eq!(RAIL_TOP_SPACE, 24.0);
        assert_eq!(RAIL_ITEM_GAP, 16.0);
        assert_eq!(TAB_STRIP_HEIGHT + TOOLBAR_HEIGHT, 166.0);
        assert_eq!(TAB_STRIP_CONTENT_ALIGN, egui::Align::Max);
        assert_eq!(FOOTER_HEIGHT, 80.0);
        assert_eq!(FOOTER_PANEL_MARGIN_X, 0);
        assert_eq!(FOOTER_PANEL_MARGIN_Y, 8);
        assert_eq!(FOOTER_LEFT_PADDING, 22.0);
        assert_eq!(FOOTER_RIGHT_PADDING, 22.0);
        assert_eq!(FOOTER_ITEM_SPACING, 18.0);
        assert_eq!(FOOTER_ICON_SIZE, 28.0);
        assert_eq!(FOOTER_PROTECTION_ICON_LABEL_GAP, 16.0);
        assert_eq!(FOOTER_PROTECTION_LABEL_WIDTH, 200.0);
        assert_eq!(FOOTER_PROTECTION_STATUS_HEIGHT, 40.0);
        assert_eq!(footer_protection_status_width(), 244.0);
        assert_eq!(FOOTER_TEXT_SIZE, 16.0);
        assert_eq!(FOOTER_SEPARATOR_HEIGHT, 28.0);
        assert_eq!(FOOTER_SYNC_DOT_SIZE, 12.0);
        assert_eq!(footer_sync_dot_radius(), 6.0);
        assert_eq!(FOOTER_SYNC_DOT_LABEL_GAP, 10.0);
        assert_eq!(FOOTER_SYNC_LABEL_WIDTH, 64.0);
        assert_eq!(FOOTER_SYNC_STATUS_HEIGHT, 40.0);
        assert_eq!(footer_sync_status_width(), 86.0);
        assert_eq!(FOOTER_SETTINGS_BUTTON_SIZE, 40.0);
        assert_eq!(FOOTER_SETTINGS_BUTTON_RADIUS, 8);
        assert_eq!(FOOTER_SETTINGS_ICON_SIZE, 28.0);
        assert_eq!(footer_settings_icon_color(), slate_theme::MUTED);
        assert_eq!(FOOTER_SETTINGS_GEAR_RADIUS, 9.0);
        assert_eq!(FOOTER_SETTINGS_GEAR_CENTER_RADIUS, 3.5);
        assert_eq!(FOOTER_SETTINGS_GEAR_TOOTH_INNER_RADIUS, 12.0);
        assert_eq!(FOOTER_SETTINGS_GEAR_TOOTH_OUTER_RADIUS, 14.0);
        assert_eq!(FOOTER_SETTINGS_GEAR_STROKE, 2.0);
        assert_eq!(ADDRESS_HEIGHT, 54.0);
        assert_eq!(ADDRESS_INPUT_TEXT_SIZE, 20.0);
        assert_eq!(ADDRESS_CORNER_RADIUS, 8);
        assert_eq!(APP_TITLE_HEIGHT, TAB_STRIP_HEIGHT);
        assert_eq!(APP_TITLE_LEFT_PADDING, 31.0);
        assert_eq!(APP_TITLE_TEXT_SIZE, 28.0);
        assert_eq!(TAB_WIDTH, 308.0);
        assert_eq!(tab_content_width(), 276.0);
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
        assert_eq!(TAB_CLOSE_BUTTON_SIZE, 28.0);
        assert_eq!(TAB_CLOSE_BUTTON_RADIUS, 6);
        assert_eq!(TAB_CLOSE_ICON_SIZE, 16.0);
        assert_eq!(NEW_TAB_LEFT_GAP, 18.0);
        assert_eq!(NEW_TAB_BUTTON_SIZE, 44.0);
        assert_eq!(NEW_TAB_BUTTON_RADIUS, 8);
        assert_eq!(NEW_TAB_ICON_SIZE, 24.0);
        assert_eq!(NEW_TAB_ICON_STROKE, 2.0);
        assert_eq!(HOME_SEARCH_MIN_WIDTH, 280.0);
        assert_eq!(HOME_SEARCH_MAX_WIDTH, 880.0);
        assert_eq!(HOME_SEARCH_HORIZONTAL_PADDING, 32.0);
        assert_eq!(HOME_SEARCH_HEIGHT, 72.0);
        assert_eq!(HOME_SEARCH_FRAME_EXTRA_HEIGHT, 8.0);
        assert_eq!(home_search_rendered_height(), 80.0);
        assert_eq!(HOME_SEARCH_TEXT_HEIGHT, 34.0);
        assert_eq!(HOME_SEARCH_INPUT_TEXT_SIZE, 20.0);
        assert_eq!(HOME_SEARCH_INNER_MARGIN_X, 32);
        assert_eq!(HOME_SEARCH_ICON_SIZE, 24.0);
        assert_eq!(HOME_SEARCH_ICON_GAP, 24.0);
        assert_eq!(HOME_SEARCH_CORNER_RADIUS, 8);
        assert_eq!(TOOLBAR_PANEL_MARGIN_X, 18);
        assert_eq!(TOOLBAR_PANEL_MARGIN_Y, 10);
        assert_eq!(TOOLBAR_ITEM_SPACING, 20.0);
        assert_eq!(TOOLBAR_BUTTON_SIZE, 40.0);
        assert_eq!(TOOLBAR_BUTTON_RADIUS, 8);
        assert_eq!(TOOLBAR_ICON_SIZE, 24.0);
        assert_eq!(TOOLBAR_PRIVACY_ICON_SIZE, 24.0);
        assert_eq!(TOOLBAR_MENU_ICON_WIDTH, 24.0);
        assert_eq!(TOOLBAR_MENU_ICON_GAP, 7.0);
        assert_eq!(TOOLBAR_MENU_ICON_STROKE, 2.0);
        assert_eq!(TOOLBAR_SEPARATOR_HEIGHT, 36.0);
        assert_eq!(TAB_ICON_SIZE, 24.0);
        assert_eq!(ADDRESS_LEADING_GAP, 28.0);
        assert_eq!(ADDRESS_INNER_MARGIN_X, 12);
        assert_eq!(ADDRESS_SHADOW_OFFSET, [0, 1]);
        assert_eq!(ADDRESS_SHADOW_BLUR, 6);
        assert_eq!(ADDRESS_SHADOW_SPREAD, 0);
        assert_eq!(ADDRESS_SHADOW_ALPHA, 10);
        assert_eq!(ADDRESS_SECURITY_ICON_SIZE, 24.0);
        assert_eq!(ADDRESS_ICON_GAP, 8.0);
        assert_eq!(ADDRESS_BOOKMARK_ICON_SIZE, 24.0);
        assert_eq!(address_bookmark_icon_color(), slate_theme::MUTED);
        assert_eq!(ADDRESS_BOOKMARK_BUTTON_SIZE, 28.0);
        assert_eq!(ADDRESS_BOOKMARK_BUTTON_RADIUS, 6);
        assert_eq!(ADDRESS_BOOKMARK_RESERVED_WIDTH, 36.0);
        assert_eq!(ADDRESS_TRAILING_CONTROLS_WIDTH, 176.0);
        assert_eq!(HOME_TOP_SPACE_FACTOR, 0.18);
        assert_eq!(HOME_TOP_SPACE_MIN, 48.0);
        assert_eq!(HOME_TOP_SPACE_MAX, 132.0);
        assert_eq!(HOME_BOTTOM_MIN_GAP, 16.0);
        assert_eq!(HOME_HERO_SIZE, 64.0);
        assert_eq!(HOME_HERO_TO_SEARCH_GAP, 44.0);
        assert_eq!(HOME_SEARCH_TO_METRICS_GAP, 62.0);
        assert_eq!(HOME_PANEL_SHADOW_OFFSET, [0, 2]);
        assert_eq!(HOME_PANEL_SHADOW_BLUR, 12);
        assert_eq!(HOME_PANEL_SHADOW_SPREAD, 0);
        assert_eq!(HOME_PANEL_SHADOW_ALPHA, 14);
        assert_eq!(HOME_METRIC_CARD_HEIGHT, 172.0);
        assert_eq!(HOME_METRIC_GRID_EXTRA_HEIGHT, 25.0);
        assert_eq!(home_metrics_rendered_height(), 197.0);
        assert_eq!(HOME_METRIC_CARD_INNER_MARGIN_X, 16);
        assert_eq!(HOME_METRIC_CARD_INNER_MARGIN_Y, 28);
        assert_eq!(HOME_METRIC_ICON_SIZE, 40.0);
        assert_eq!(HOME_METRIC_ICON_LABEL_GAP, 18.0);
        assert_eq!(HOME_METRIC_LABEL_TEXT_SIZE, 17.0);
        assert_eq!(HOME_METRIC_DETAIL_TEXT_SIZE, 14.0);
        assert_eq!(HOME_METRIC_DETAIL_GAP, 4.0);
        assert_eq!(HOME_METRIC_BADGE_TEXT_SIZE, 13.0);
        assert_eq!(HOME_METRIC_BADGE_LABEL_GAP, 8.0);
        assert_eq!(HOME_METRIC_BADGE_MARGIN_X, 8);
        assert_eq!(HOME_METRIC_BADGE_MARGIN_Y, 3);
        assert_eq!(HOME_METRIC_BADGE_CORNER_RADIUS, 10);
        assert!((home_metric_badge_width("23") - 31.08).abs() < 0.001);
        assert!((home_metric_badge_width("184") - 38.62).abs() < 0.001);
        assert_eq!(STATUS_BUBBLE_MARGIN_X, 14.0);
        assert_eq!(STATUS_BUBBLE_MARGIN_Y, 12.0);
        assert_eq!(STATUS_BUBBLE_HEIGHT, 32.0);
        assert_eq!(STATUS_BUBBLE_MAX_WIDTH, 560.0);
        assert_eq!(STATUS_BUBBLE_HORIZONTAL_PADDING, 12.0);
        assert_eq!(STATUS_BUBBLE_CORNER_RADIUS, 8);
        assert_eq!(STATUS_TEXT_SIZE, 13.0);
    }

    #[test]
    fn wide_toolbar_address_width_leaves_room_for_trailing_controls() {
        assert_eq!(toolbar_address_width(1348.0), 1172.0);
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
    fn address_security_icon_uses_slate_shield_for_home() {
        assert_eq!(
            address_security_icon_for_location("slate://home"),
            AddressSecurityIcon::Slate {
                icon: slate_theme::SlateIcon::TopShield,
                color: slate_theme::MUTED,
            }
        );
    }

    #[test]
    fn address_security_icon_reflects_common_url_schemes() {
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
        assert_eq!(home_search_width(1200.0), HOME_SEARCH_MAX_WIDTH);
        assert_eq!(home_search_width(620.0), 588.0);
        assert_eq!(home_search_width(250.0), 218.0);
    }

    #[test]
    fn tab_title_width_reserves_fixed_close_region() {
        assert_eq!(tab_title_width(tab_content_width()), 204.0);
        assert_eq!(tab_title_width(100.0), TAB_TITLE_MIN_WIDTH);
    }

    #[test]
    fn tab_chrome_colors_mute_inactive_tabs() {
        assert_eq!(tab_title_color(true), slate_theme::TEXT);
        assert_eq!(tab_title_color(false), slate_theme::MUTED);
        assert_eq!(tab_close_icon_color(true), slate_theme::TEXT);
        assert_eq!(tab_close_icon_color(false), slate_theme::MUTED);
    }

    #[test]
    fn toolbar_navigation_colors_mute_disabled_controls() {
        assert_eq!(toolbar_navigation_icon_color(true), slate_theme::TEXT);
        assert_eq!(toolbar_navigation_icon_color(false), slate_theme::MUTED);
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
        assert_eq!(home_metric_card_content_height(), 116.0);
    }

    #[test]
    fn headless_home_content_fits_default_opening_view() {
        let viewport_size = default_opening_home_view_size();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size);
        let layout = render_home_content_layout(viewport_size);

        for rect in [layout.hero_rect, layout.search_rect, layout.metrics_rect] {
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
        let center_x = viewport_size.x / 2.0;

        for rect in [layout.hero_rect, layout.search_rect] {
            assert!(
                (rect.center().x - center_x).abs() < LAYOUT_EPSILON,
                "expected {rect:?} to stay centered in {viewport_size:?}"
            );
        }
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
            (225.0..=242.0).contains(&layout.search_rect.top()),
            "expected search to sit near the screenshot vertical rhythm: {:?}",
            layout.search_rect
        );
        assert!(
            (365.0..=390.0).contains(&layout.metrics_rect.top()),
            "expected metric cards to sit near the screenshot vertical rhythm: {:?}",
            layout.metrics_rect
        );
        assert!(
            (330.0..=360.0).contains(&layout.metrics_rect.left()),
            "expected metric cards to start near the screenshot horizontal rhythm: {:?}",
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
