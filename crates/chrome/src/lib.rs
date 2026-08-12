#![forbid(unsafe_code)]

use fontdb::{Database, Family, Query};
use fontdue::{Font, FontSettings};
use slate_apps::{AppDescriptor, AppIcon, AppId, default_apps};
use slate_browser_core::BrowserState;
use slate_rendering::{MetricAccent, RenderMetric};

const BG: u32 = 0x00FBFAF8;
const SURFACE: u32 = 0x00FFFFFF;
const PANEL: u32 = 0x00F4F2EF;
const BORDER: u32 = 0x00DDD9D4;
const TEXT: u32 = 0x00272727;
const MUTED: u32 = 0x006F6B67;
const TEAL: u32 = 0x000B6B68;
const TEAL_SOFT: u32 = 0x00E5F0EE;
const AMBER: u32 = 0x00D99A00;
const BLUE: u32 = 0x001D74C9;
const SHADOW: u32 = 0x00ECEAE5;
const TAB_H: usize = 58;
const TOOLBAR_H: usize = 68;
const RAIL_W: usize = 84;
const FOOTER_H: usize = 60;
const TAB_X: usize = 126;
const TAB_Y: usize = 11;
const WINDOW_CONTROL_Y: usize = 16;
const WINDOW_CONTROL_W: usize = 38;
const WINDOW_CONTROL_H: usize = 30;
const WINDOW_CONTROL_GAP: usize = 12;
const NAV_ICON_SIZE: usize = 32;
const TAB_ICON_SIZE: usize = 20;
const APP_ICON_SIZE: usize = 34;
const TOP_SHIELD_ICON_SIZE: usize = 28;
const NAV_BACK_ICON_MASK: &[u8] = include_bytes!("../assets/icons/nav_back.alpha");
const NAV_FORWARD_ICON_MASK: &[u8] = include_bytes!("../assets/icons/nav_forward.alpha");
const NAV_REFRESH_ICON_MASK: &[u8] = include_bytes!("../assets/icons/nav_refresh.alpha");
const TOP_SHIELD_ICON_MASK: &[u8] = include_bytes!("../assets/icons/top_shield.alpha");
const WEB_ICON_MASK: &[u8] = include_bytes!("../assets/icons/web.alpha");
const DOWNLOADS_ICON_MASK: &[u8] = include_bytes!("../assets/icons/downloads.alpha");
const CALENDAR_ICON_MASK: &[u8] = include_bytes!("../assets/icons/calendar.alpha");
const MESSAGING_ICON_MASK: &[u8] = include_bytes!("../assets/icons/messaging.alpha");
const TAB_WEB_ICON_MASK: &[u8] = include_bytes!("../assets/icons/tab_web.alpha");
const TAB_RESEARCH_ICON_MASK: &[u8] = include_bytes!("../assets/icons/tab_research.alpha");
const TAB_CALENDAR_ICON_MASK: &[u8] = include_bytes!("../assets/icons/tab_calendar.alpha");
const HOME_HERO_SHIELD_SIZE: usize = 64;
const HOME_SEARCH_ICON_SIZE: usize = 32;
const HOME_METRIC_ICON_SIZE: usize = 40;
const HOME_FOOTER_SHIELD_SIZE: usize = 28;
const HOME_HERO_SHIELD_MASK: &[u8] = include_bytes!("../assets/icons/home_hero_shield.alpha");
const HOME_SEARCH_ICON_MASK: &[u8] = include_bytes!("../assets/icons/home_search.alpha");
const HOME_METRIC_PRIVACY_MASK: &[u8] = include_bytes!("../assets/icons/home_metric_privacy.alpha");
const HOME_METRIC_LOCK_MASK: &[u8] = include_bytes!("../assets/icons/home_metric_lock.alpha");
const HOME_METRIC_ADS_MASK: &[u8] = include_bytes!("../assets/icons/home_metric_ads.alpha");
const HOME_METRIC_TIME_MASK: &[u8] = include_bytes!("../assets/icons/home_metric_time.alpha");
const HOME_FOOTER_SHIELD_MASK: &[u8] = include_bytes!("../assets/icons/home_footer_shield.alpha");

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowVisualState {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowCommand {
    #[default]
    None,
    Close,
    Minimize,
    ToggleMaximize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

struct TextRenderer {
    font: Option<Font>,
}

impl TextRenderer {
    fn load() -> Self {
        let mut database = Database::new();
        database.load_system_fonts();
        let query = Query {
            families: &[Family::SansSerif],
            ..Query::default()
        };
        let font = database
            .query(&query)
            .and_then(|id| {
                database.with_face_data(id, |font_data, _face_index| {
                    Font::from_bytes(font_data.to_vec(), FontSettings::default()).ok()
                })
            })
            .flatten();

        Self { font }
    }

    fn draw_text(
        &self,
        frame: &mut Frame,
        x: usize,
        y: usize,
        size: usize,
        text: &str,
        color: u32,
        max_width: Option<usize>,
    ) -> bool {
        let Some(font) = &self.font else {
            return false;
        };

        let font_size = usize_to_f32(size);
        let baseline = cx_i32(y).saturating_add(
            font.horizontal_line_metrics(font_size)
                .map(|metrics| f32_to_i32_ceil(metrics.ascent))
                .unwrap_or(cx_i32(size)),
        );
        let max_x = max_width
            .map(|width| x.saturating_add(width))
            .unwrap_or(usize::MAX);
        let mut cursor = x;

        for ch in text.chars() {
            let advance = if ch.is_whitespace() {
                whitespace_advance(size)
            } else {
                let (metrics, bitmap) = font.rasterize(ch, font_size);
                let advance = f32_to_usize_ceil(metrics.advance_width).max(size / 3);
                if cursor.saturating_add(advance) > max_x {
                    break;
                }

                let glyph_y = baseline.saturating_add(f32_to_i32_floor(
                    -metrics.bounds.height - metrics.bounds.ymin,
                ));

                for row in 0..metrics.height {
                    for col in 0..metrics.width {
                        let index = row.saturating_mul(metrics.width).saturating_add(col);
                        let Some(alpha) = bitmap.get(index).copied() else {
                            continue;
                        };
                        if alpha == 0 {
                            continue;
                        }

                        let px = cx_i32(cursor)
                            .saturating_add(metrics.xmin)
                            .saturating_add(cx_i32(col));
                        let py = glyph_y.saturating_add(cx_i32(row));
                        frame.blend_pixel_i32(px, py, color, alpha);
                    }
                }

                advance
            };

            cursor = cursor.saturating_add(advance);
            if cursor > max_x {
                break;
            }
        }

        true
    }

    fn measure_width(&self, size: usize, text: &str) -> Option<usize> {
        let font = self.font.as_ref()?;
        let font_size = usize_to_f32(size);
        let mut width = 0_usize;

        for ch in text.chars() {
            let advance = if ch.is_whitespace() {
                whitespace_advance(size)
            } else {
                let (metrics, _) = font.rasterize(ch, font_size);
                f32_to_usize_ceil(metrics.advance_width).max(size / 3)
            };
            width = width.saturating_add(advance);
        }

        Some(width)
    }
}

pub struct ChromeView {
    state: BrowserState,
    apps: Vec<AppDescriptor>,
    text: TextRenderer,
    window_state: WindowVisualState,
}

impl ChromeView {
    pub fn new(state: BrowserState) -> Self {
        Self {
            state,
            apps: default_apps().to_vec(),
            text: TextRenderer::load(),
            window_state: WindowVisualState::Normal,
        }
    }

    pub fn state(&self) -> &BrowserState {
        &self.state
    }

    pub fn set_window_state(&mut self, state: WindowVisualState) {
        self.window_state = state;
    }

    pub fn handle_click(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowCommand {
        if let Some(control) = self.hit_window_control(x, y, width) {
            return match control {
                WindowControl::Minimize => WindowCommand::Minimize,
                WindowControl::Maximize => WindowCommand::ToggleMaximize,
                WindowControl::Close => WindowCommand::Close,
            };
        }

        if self.window_state == WindowVisualState::Minimized {
            return WindowCommand::None;
        }

        if let Some(app) = self.hit_app(x, y, height) {
            self.state.select_app(app);
            return WindowCommand::None;
        }

        if let Some(tab) = self.hit_tab(x, y, width) {
            let _ = self.state.activate_tab(tab);
            return WindowCommand::None;
        }

        if self.hit_new_tab(x, y, width) {
            self.state.add_mock_tab();
        }

        WindowCommand::None
    }

    pub fn is_draggable_chrome(&self, x: usize, y: usize, width: usize, _height: usize) -> bool {
        y < TAB_H
            && self.hit_window_control(x, y, width).is_none()
            && self.hit_tab(x, y, width).is_none()
            && !self.hit_new_tab(x, y, width)
    }

    pub fn render(&self, width: usize, height: usize) -> Frame {
        let width = width.max(320);
        let height = height.max(240);
        let mut frame = Frame::new(width, height, BG);
        let mut canvas = Canvas::new(&mut frame, &self.text);

        if self.window_state == WindowVisualState::Minimized {
            self.draw_minimized_window(&mut canvas, width, height);
        } else {
            self.draw_window(&mut canvas, width, height);
        }

        frame
    }

    fn visible_tab_count(&self) -> usize {
        self.state.tabs.len().min(5)
    }

    fn tab_width(&self, width: usize) -> usize {
        let count = self.visible_tab_count().max(1);
        let available = width.saturating_sub(TAB_X + 150);
        (available / count).clamp(124, 220)
    }

    fn hit_tab(&self, x: usize, y: usize, width: usize) -> Option<usize> {
        if !(TAB_Y..TAB_Y + 46).contains(&y) || x < TAB_X {
            return None;
        }

        let tab_w = self.tab_width(width);
        for index in 0..self.visible_tab_count() {
            let tx = TAB_X + index * (tab_w + 2);
            if (tx..tx + tab_w).contains(&x) {
                return Some(index);
            }
        }

        None
    }

    fn hit_new_tab(&self, x: usize, y: usize, width: usize) -> bool {
        if !(TAB_Y..TAB_Y + 46).contains(&y) {
            return false;
        }

        let tab_w = self.tab_width(width);
        let plus_x = TAB_X + self.visible_tab_count() * (tab_w + 2) + 12;
        (plus_x..plus_x + 44).contains(&x)
    }

    fn hit_app(&self, x: usize, y: usize, height: usize) -> Option<AppId> {
        if x >= RAIL_W {
            return None;
        }

        let top = TAB_H;
        let bottom = height.saturating_sub(FOOTER_H);
        if !(top..bottom).contains(&y) {
            return None;
        }

        for (index, app) in self.apps.iter().enumerate() {
            let item_y = top + 22 + index * 72;
            if (item_y.saturating_sub(8)..item_y + 58).contains(&y) {
                return Some(app.id);
            }
        }

        None
    }

    fn hit_window_control(&self, x: usize, y: usize, width: usize) -> Option<WindowControl> {
        if !(WINDOW_CONTROL_Y..WINDOW_CONTROL_Y + WINDOW_CONTROL_H).contains(&y) {
            return None;
        }

        let [minimize_x, maximize_x, close_x] = window_control_positions(width);

        if (minimize_x..minimize_x + WINDOW_CONTROL_W).contains(&x) {
            Some(WindowControl::Minimize)
        } else if (maximize_x..maximize_x + WINDOW_CONTROL_W).contains(&x) {
            Some(WindowControl::Maximize)
        } else if (close_x..close_x + WINDOW_CONTROL_W).contains(&x) {
            Some(WindowControl::Close)
        } else {
            None
        }
    }

    fn draw_window(&self, canvas: &mut Canvas<'_>, width: usize, height: usize) {
        let top_h = TAB_H + TOOLBAR_H;
        let footer_y = height.saturating_sub(FOOTER_H);

        canvas.rect(0, 0, width, height, BG);
        canvas.rect(0, 0, width, TAB_H, SURFACE);
        canvas.rect(
            RAIL_W,
            TAB_H,
            width.saturating_sub(RAIL_W),
            TOOLBAR_H,
            SURFACE,
        );
        canvas.hline(0, TAB_H, width, BORDER);
        canvas.hline(RAIL_W, top_h, width.saturating_sub(RAIL_W), BORDER);

        canvas.draw_text(22, 19, 3, "Slate", TEXT);
        self.draw_window_controls(canvas, width);
        self.draw_tabs(canvas, TAB_X, TAB_Y, width);
        self.draw_toolbar(canvas, RAIL_W, TAB_H, width, TOOLBAR_H);
        self.draw_app_rail(canvas, RAIL_W, TAB_H, footer_y.saturating_sub(TAB_H));
        self.draw_content(
            canvas,
            RAIL_W,
            top_h,
            width.saturating_sub(RAIL_W),
            footer_y.saturating_sub(top_h),
        );
        self.draw_footer(
            canvas,
            RAIL_W,
            footer_y,
            width.saturating_sub(RAIL_W),
            FOOTER_H,
        );
    }

    fn draw_minimized_window(&self, canvas: &mut Canvas<'_>, width: usize, height: usize) {
        canvas.rect(0, 0, width, height, BG);
        canvas.rect(0, 0, width, TAB_H, SURFACE);
        canvas.hline(0, TAB_H, width, BORDER);
        canvas.draw_text(22, 19, 3, "Slate", TEXT);
        self.draw_window_controls(canvas, width);
        self.draw_shield(canvas, 42, TAB_H + 34, 14, TEAL);
        canvas.draw_text_clipped(72, TAB_H + 24, 2, "Slate", TEXT, width.saturating_sub(150));
    }

    fn draw_window_controls(&self, canvas: &mut Canvas<'_>, width: usize) {
        let [minimize_x, maximize_x, close_x] = window_control_positions(width);
        self.draw_window_button(canvas, minimize_x, WindowControl::Minimize);
        self.draw_window_button(canvas, maximize_x, WindowControl::Maximize);
        self.draw_window_button(canvas, close_x, WindowControl::Close);
    }

    fn draw_window_button(&self, canvas: &mut Canvas<'_>, x: usize, control: WindowControl) {
        let y = WINDOW_CONTROL_Y;
        let color = TEXT;

        match control {
            WindowControl::Minimize => {
                canvas.hline(x + 12, y + 15, 14, color);
            }
            WindowControl::Maximize => {
                if self.window_state == WindowVisualState::Maximized {
                    canvas.rect_border(x + 10, y + 10, 12, 10, color);
                    canvas.rect_border(x + 14, y + 7, 12, 10, color);
                } else {
                    canvas.rect_border(x + 12, y + 7, 14, 14, color);
                }
            }
            WindowControl::Close => {
                canvas.line_i32(
                    cx_i32(x + 12),
                    cx_i32(y + 8),
                    cx_i32(x + 25),
                    cx_i32(y + 21),
                    color,
                );
                canvas.line_i32(
                    cx_i32(x + 25),
                    cx_i32(y + 8),
                    cx_i32(x + 12),
                    cx_i32(y + 21),
                    color,
                );
            }
        }
    }

    fn draw_tabs(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, width: usize) {
        let tab_count = self.visible_tab_count();
        let tab_w = self.tab_width(width);
        for (index, tab) in self.state.tabs.iter().take(tab_count).enumerate() {
            let tx = x + index * (tab_w + 2);
            let color = if index == self.state.active_tab {
                SURFACE
            } else {
                PANEL
            };
            canvas.rounded_rect_panel(tx, y, tab_w, 47, 9, BORDER, color);
            let icon_color = if index == self.state.active_tab {
                TEAL
            } else {
                MUTED
            };
            self.draw_small_tab_icon(canvas, tx + 18, y + 15, icon_color, index);
            canvas.draw_text_clipped(
                tx + 42,
                y + 17,
                2,
                &tab.title,
                TEXT,
                tab_w.saturating_sub(74),
            );
            let close_x = tx + tab_w.saturating_sub(28);
            canvas.line_i32(
                cx_i32(close_x),
                cx_i32(y + 17),
                cx_i32(close_x + 9),
                cx_i32(y + 26),
                MUTED,
            );
            canvas.line_i32(
                cx_i32(close_x + 9),
                cx_i32(y + 17),
                cx_i32(close_x),
                cx_i32(y + 26),
                MUTED,
            );
        }
        let plus_x = x + tab_count * (tab_w + 2) + 18;
        canvas.hline(plus_x, y + 23, 16, TEXT);
        canvas.vline(plus_x + 8, y + 15, 17, TEXT);
    }

    fn draw_toolbar(
        &self,
        canvas: &mut Canvas<'_>,
        rail_w: usize,
        tab_h: usize,
        width: usize,
        toolbar_h: usize,
    ) {
        let y = tab_h;
        canvas.vline(rail_w, y, toolbar_h, BORDER);
        canvas.draw_alpha_mask_centered(
            rail_w + 34,
            y + 34,
            NAV_ICON_SIZE,
            NAV_ICON_SIZE,
            NAV_BACK_ICON_MASK,
            TEXT,
        );
        canvas.draw_alpha_mask_centered(
            rail_w + 78,
            y + 34,
            NAV_ICON_SIZE,
            NAV_ICON_SIZE,
            NAV_FORWARD_ICON_MASK,
            MUTED,
        );
        canvas.draw_alpha_mask_centered(
            rail_w + 124,
            y + 34,
            NAV_ICON_SIZE,
            NAV_ICON_SIZE,
            NAV_REFRESH_ICON_MASK,
            TEXT,
        );

        let address_x = rail_w + 170;
        let address_w = width.saturating_sub(address_x + 130).max(160);
        canvas.rounded_rect_panel(address_x, y + 15, address_w, 42, 8, BORDER, SURFACE);
        self.draw_shield(canvas, address_x + 23, y + 36, 11, MUTED);
        canvas.draw_text_clipped(
            address_x + 52,
            y + 27,
            2,
            &self.state.surface.address,
            TEXT,
            address_w.saturating_sub(98),
        );
        self.draw_star_icon(
            canvas,
            address_x + address_w.saturating_sub(28),
            y + 36,
            MUTED,
        );
        canvas.draw_alpha_mask_centered(
            width.saturating_sub(94),
            y + 36,
            TOP_SHIELD_ICON_SIZE,
            TOP_SHIELD_ICON_SIZE,
            TOP_SHIELD_ICON_MASK,
            AMBER,
        );
        canvas.vline(width.saturating_sub(68), y + 23, 27, BORDER);
        self.draw_menu_icon(canvas, width.saturating_sub(38), y + 36, TEXT);
    }

    fn draw_app_rail(&self, canvas: &mut Canvas<'_>, rail_w: usize, y: usize, h: usize) {
        canvas.rect(0, y, rail_w, h, SURFACE);
        canvas.vline(rail_w, y, h, BORDER);

        for (index, app) in self.apps.iter().enumerate() {
            let item_y = y + 22 + index * 72;
            let active = app.id == self.state.active_app;
            if active {
                canvas.rounded_rect(10, item_y.saturating_sub(6), 62, 60, 9, TEAL_SOFT);
            }
            let color = if active { TEAL } else { TEXT };
            self.draw_app_icon(canvas, app.icon, rail_w / 2, item_y + 20, color);
        }
    }

    fn draw_content(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, h: usize) {
        match self.state.active_app {
            AppId::Web => self.draw_home(canvas, x, y, w, h),
            AppId::Downloads => self.draw_downloads(canvas, x, y, w, h),
            AppId::Calendar => self.draw_calendar(canvas, x, y, w, h),
            AppId::Messaging => self.draw_messaging(canvas, x, y, w, h),
        }
    }

    fn draw_home(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, h: usize) {
        canvas.rect(x, y, w, h, BG);

        let center_x = x + w / 2;
        let shield_y = y + (h.saturating_mul(23) / 100).max(74);
        canvas.draw_alpha_mask_centered(
            center_x,
            shield_y,
            HOME_HERO_SHIELD_SIZE,
            HOME_HERO_SHIELD_SIZE,
            HOME_HERO_SHIELD_MASK,
            TEAL,
        );

        let search_w = (w.saturating_mul(58) / 100)
            .clamp(560, 1020)
            .min(w.saturating_sub(150));
        let search_h = 58;
        let search_x = center_x.saturating_sub(search_w / 2);
        let search_y = shield_y + 72;
        canvas.rounded_rect_panel(search_x, search_y, search_w, search_h, 9, BORDER, SURFACE);
        canvas.draw_alpha_mask_centered(
            search_x + 38,
            search_y + 31,
            HOME_SEARCH_ICON_SIZE,
            HOME_SEARCH_ICON_SIZE,
            HOME_SEARCH_ICON_MASK,
            MUTED,
        );
        canvas.draw_text_clipped(
            search_x + 78,
            search_y + 22,
            2,
            "Search the web or enter an address",
            MUTED,
            search_w.saturating_sub(104),
        );

        let metrics = &self.state.surface.metrics;
        let gap = 26;
        let max_cards_w = (w.saturating_mul(56) / 100)
            .clamp(620, 970)
            .min(w.saturating_sub(140));
        let card_w = ((max_cards_w.saturating_sub(gap * metrics.len().saturating_sub(1)))
            / metrics.len().max(1))
        .clamp(126, 224);
        let card_h = (h.saturating_mul(24) / 100).clamp(130, 190);
        let total_w = metrics.len() * card_w + metrics.len().saturating_sub(1) * gap;
        let start_x = center_x.saturating_sub(total_w / 2);
        let card_y = search_y + 102;

        for (index, metric) in metrics.iter().enumerate() {
            let card_x = start_x + index * (card_w + gap);
            self.draw_metric_card(canvas, card_x, card_y, card_w, card_h, metric, index);
        }
    }

    fn draw_metric_card(
        &self,
        canvas: &mut Canvas<'_>,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        metric: &RenderMetric,
        index: usize,
    ) {
        canvas.rounded_rect(x + 2, y + 2, w, h, 8, SHADOW);
        canvas.rounded_rect_panel(x, y, w, h, 8, BORDER, SURFACE);

        let color = metric_color(metric.accent);
        self.draw_metric_icon(canvas, x + w / 2, y + 44, color, index);

        let label_y = y + h.saturating_sub(54);
        if metric.value.is_empty() {
            canvas.draw_text_centered(
                x + 14,
                label_y,
                1,
                &metric.label,
                TEXT,
                w.saturating_sub(28),
            );
            return;
        }

        if metric.value.contains('h') {
            canvas.draw_text_centered(
                x + 14,
                label_y,
                1,
                &metric.label,
                TEXT,
                w.saturating_sub(28),
            );
            canvas.draw_text_centered(
                x + 14,
                label_y + 23,
                1,
                &metric.value,
                MUTED,
                w.saturating_sub(28),
            );
            return;
        }

        let badge_gap = 6;
        let badge_w = canvas.measure_text(1, &metric.value).saturating_add(16);
        let available_label_w = w.saturating_sub(badge_w + badge_gap).max(42);
        let label_w = canvas
            .measure_text(1, &metric.label)
            .saturating_add(4)
            .min(available_label_w);
        let total_w = label_w.saturating_add(badge_w).saturating_add(badge_gap);
        let start_x = x + w.saturating_sub(total_w) / 2;
        canvas.draw_text_clipped(start_x, label_y, 1, &metric.label, TEXT, label_w);
        let badge_x = start_x.saturating_add(label_w).saturating_add(badge_gap);
        canvas.rounded_rect(badge_x, label_y.saturating_sub(3), badge_w, 20, 10, color);
        canvas.draw_text_centered(
            badge_x + 6,
            label_y + 2,
            1,
            &metric.value,
            SURFACE,
            badge_w.saturating_sub(12),
        );
    }

    fn draw_downloads(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, h: usize) {
        self.draw_app_header(
            canvas,
            x,
            y,
            w,
            "Downloads",
            "Local queue and saved broadweb files",
        );
        let list_x = x + 72;
        let list_y = y + 104;
        let list_w = w.saturating_sub(144).min(860);
        self.draw_list_row(
            canvas,
            list_x,
            list_y,
            list_w,
            "No active downloads",
            "Queue is clear",
            TEAL,
        );
        self.draw_list_row(
            canvas,
            list_x,
            list_y + 72,
            list_w,
            "Pinned IPFS bundle",
            "/ip4/127.0.0.1/tcp/8080/http",
            BLUE,
        );
        self.draw_list_row(
            canvas,
            list_x,
            list_y + 144,
            list_w,
            "Private route check",
            "Tor and I2P proxies not connected",
            AMBER,
        );
        self.draw_status_band(
            canvas,
            x + 72,
            y + h.saturating_sub(96),
            list_w,
            "Downloads are profile-scoped",
        );
    }

    fn draw_calendar(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, h: usize) {
        self.draw_app_header(canvas, x, y, w, "Calendar", "Local-first planning surface");
        let grid_x = x + 72;
        let grid_y = y + 112;
        let cell = ((w.saturating_sub(180)) / 7).clamp(42, 84);

        for (index, day) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
            canvas.draw_text(grid_x + index * cell + 16, grid_y, 2, day, MUTED);
        }

        for row in 0..5 {
            for col in 0..7 {
                let cx = grid_x + col * cell;
                let cy = grid_y + 28 + row * 58;
                canvas.rect(cx, cy, cell.saturating_sub(8), 48, SURFACE);
                canvas.rect_border(cx, cy, cell.saturating_sub(8), 48, BORDER);
                let day = row * 7 + col + 1;
                let color = if day == 11 { TEAL } else { TEXT };
                canvas.draw_text(cx + 12, cy + 16, 2, &day.to_string(), color);
                if day == 11 {
                    canvas.rect(cx + 2, cy + 44, cell.saturating_sub(12), 3, TEAL);
                }
            }
        }

        self.draw_status_band(
            canvas,
            x + 72,
            y + h.saturating_sub(96),
            520.min(w.saturating_sub(144)),
            "No remote calendar sync enabled",
        );
    }

    fn draw_messaging(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, h: usize) {
        self.draw_app_header(canvas, x, y, w, "Messaging", "Private conversation surface");
        let sidebar_w = 290.min(w.saturating_sub(160));
        let sidebar_x = x + 72;
        let pane_y = y + 108;
        let pane_h = h.saturating_sub(164);
        canvas.rect(sidebar_x, pane_y, sidebar_w, pane_h, SURFACE);
        canvas.rect_border(sidebar_x, pane_y, sidebar_w, pane_h, BORDER);
        self.draw_list_row(
            canvas,
            sidebar_x + 16,
            pane_y + 18,
            sidebar_w.saturating_sub(32),
            "Broadweb Notes",
            "2 unread",
            TEAL,
        );
        self.draw_list_row(
            canvas,
            sidebar_x + 16,
            pane_y + 90,
            sidebar_w.saturating_sub(32),
            "Servo Patch",
            "Draft PR notes",
            BLUE,
        );

        let thread_x = sidebar_x + sidebar_w + 28;
        let thread_w = w.saturating_sub(thread_x.saturating_sub(x) + 72);
        canvas.rect(thread_x, pane_y, thread_w, pane_h, SURFACE);
        canvas.rect_border(thread_x, pane_y, thread_w, pane_h, BORDER);
        canvas.draw_text_clipped(
            thread_x + 24,
            pane_y + 22,
            2,
            "Broadweb Notes",
            TEXT,
            thread_w.saturating_sub(48),
        );
        self.draw_message_bubble(
            canvas,
            thread_x + 24,
            pane_y + 68,
            thread_w.saturating_sub(80),
            "Route IPFS through local gateway only.",
            TEAL_SOFT,
        );
        self.draw_message_bubble(
            canvas,
            thread_x + 64,
            pane_y + 132,
            thread_w.saturating_sub(120),
            "No public fallback without consent.",
            PANEL,
        );
    }

    fn draw_app_header(
        &self,
        canvas: &mut Canvas<'_>,
        x: usize,
        y: usize,
        w: usize,
        title: &str,
        subtitle: &str,
    ) {
        canvas.rect(x, y, w, 88, BG);
        canvas.draw_text(x + 72, y + 32, 3, title, TEXT);
        canvas.draw_text_clipped(x + 72, y + 62, 2, subtitle, MUTED, w.saturating_sub(144));
        canvas.hline(x + 72, y + 88, w.saturating_sub(144), BORDER);
    }

    fn draw_list_row(
        &self,
        canvas: &mut Canvas<'_>,
        x: usize,
        y: usize,
        w: usize,
        title: &str,
        detail: &str,
        accent: u32,
    ) {
        canvas.rect(x, y, w, 56, SURFACE);
        canvas.rect_border(x, y, w, 56, BORDER);
        canvas.rect(x, y, 4, 56, accent);
        canvas.draw_text_clipped(x + 20, y + 12, 2, title, TEXT, w.saturating_sub(40));
        canvas.draw_text_clipped(x + 20, y + 34, 1, detail, MUTED, w.saturating_sub(40));
    }

    fn draw_message_bubble(
        &self,
        canvas: &mut Canvas<'_>,
        x: usize,
        y: usize,
        w: usize,
        text: &str,
        color: u32,
    ) {
        let bubble_w = w.clamp(220, 460);
        canvas.rect(x, y, bubble_w, 42, color);
        canvas.rect_border(x, y, bubble_w, 42, BORDER);
        canvas.draw_text_clipped(x + 16, y + 15, 1, text, TEXT, bubble_w.saturating_sub(32));
    }

    fn draw_status_band(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, text: &str) {
        canvas.rect(x, y, w, 44, TEAL_SOFT);
        canvas.rect_border(x, y, w, 44, BORDER);
        self.draw_shield(canvas, x + 28, y + 22, 11, TEAL);
        canvas.draw_text_clipped(x + 54, y + 16, 2, text, TEXT, w.saturating_sub(72));
    }

    fn draw_footer(&self, canvas: &mut Canvas<'_>, x: usize, y: usize, w: usize, h: usize) {
        canvas.rect(x, y, w, h, SURFACE);
        canvas.hline(x, y, w, BORDER);
        canvas.draw_alpha_mask_centered(
            x + 34,
            y + 31,
            HOME_FOOTER_SHIELD_SIZE,
            HOME_FOOTER_SHIELD_SIZE,
            HOME_FOOTER_SHIELD_MASK,
            TEAL,
        );
        canvas.draw_text(x + 64, y + 24, 2, &self.state.status.privacy, TEXT);
        canvas.circle_fill(x + w.saturating_sub(174), y + 31, 6, TEAL);
        canvas.draw_text(
            x + w.saturating_sub(154),
            y + 24,
            2,
            &self.state.status.sync,
            TEXT,
        );
        canvas.vline(x + w.saturating_sub(72), y + 18, 28, BORDER);
        self.draw_gear_icon(canvas, x + w.saturating_sub(36), y + 31, MUTED);
    }

    fn draw_small_tab_icon(
        &self,
        canvas: &mut Canvas<'_>,
        x: usize,
        y: usize,
        color: u32,
        index: usize,
    ) {
        canvas.draw_alpha_mask_centered(
            x + 8,
            y + 8,
            TAB_ICON_SIZE,
            TAB_ICON_SIZE,
            tab_icon_mask(index),
            color,
        );
    }

    fn draw_app_icon(
        &self,
        canvas: &mut Canvas<'_>,
        icon: AppIcon,
        cx: usize,
        cy: usize,
        color: u32,
    ) {
        canvas.draw_alpha_mask_centered(
            cx,
            cy,
            APP_ICON_SIZE,
            APP_ICON_SIZE,
            app_icon_mask(icon),
            color,
        );
    }

    fn draw_metric_icon(
        &self,
        canvas: &mut Canvas<'_>,
        cx: usize,
        cy: usize,
        color: u32,
        index: usize,
    ) {
        canvas.draw_alpha_mask_centered(
            cx,
            cy,
            HOME_METRIC_ICON_SIZE,
            HOME_METRIC_ICON_SIZE,
            home_metric_icon_mask(index),
            color,
        );
    }

    fn draw_star_icon(&self, canvas: &mut Canvas<'_>, cx: usize, cy: usize, color: u32) {
        let center_x = cx_i32(cx);
        let center_y = cx_i32(cy);
        let points = [
            (0, -10),
            (3, -3),
            (10, -3),
            (5, 2),
            (7, 9),
            (0, 5),
            (-7, 9),
            (-5, 2),
            (-10, -3),
            (-3, -3),
            (0, -10),
        ];

        for pair in points.windows(2) {
            let [(x0, y0), (x1, y1)] = pair else {
                continue;
            };
            canvas.stroke_line_i32(
                center_x.saturating_add(*x0),
                center_y.saturating_add(*y0),
                center_x.saturating_add(*x1),
                center_y.saturating_add(*y1),
                2,
                color,
            );
        }
    }

    fn draw_menu_icon(&self, canvas: &mut Canvas<'_>, cx: usize, cy: usize, color: u32) {
        canvas.stroke_line_i32(
            cx_i32(cx.saturating_sub(10)),
            cx_i32(cy.saturating_sub(8)),
            cx_i32(cx + 10),
            cx_i32(cy.saturating_sub(8)),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx.saturating_sub(10)),
            cx_i32(cy),
            cx_i32(cx + 10),
            cx_i32(cy),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx.saturating_sub(10)),
            cx_i32(cy + 8),
            cx_i32(cx + 10),
            cx_i32(cy + 8),
            2,
            color,
        );
    }

    fn draw_gear_icon(&self, canvas: &mut Canvas<'_>, cx: usize, cy: usize, color: u32) {
        canvas.stroke_circle_i32(cx_i32(cx), cx_i32(cy), 13, 2, color);
        canvas.stroke_circle_i32(cx_i32(cx), cx_i32(cy), 5, 2, color);
        canvas.stroke_line_i32(
            cx_i32(cx.saturating_sub(17)),
            cx_i32(cy),
            cx_i32(cx.saturating_sub(10)),
            cx_i32(cy),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx + 10),
            cx_i32(cy),
            cx_i32(cx + 17),
            cx_i32(cy),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx),
            cx_i32(cy.saturating_sub(17)),
            cx_i32(cx),
            cx_i32(cy.saturating_sub(10)),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx),
            cx_i32(cy + 10),
            cx_i32(cx),
            cx_i32(cy + 17),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx.saturating_sub(12)),
            cx_i32(cy.saturating_sub(12)),
            cx_i32(cx.saturating_sub(8)),
            cx_i32(cy.saturating_sub(8)),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx + 12),
            cx_i32(cy.saturating_sub(12)),
            cx_i32(cx + 8),
            cx_i32(cy.saturating_sub(8)),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx.saturating_sub(12)),
            cx_i32(cy + 12),
            cx_i32(cx.saturating_sub(8)),
            cx_i32(cy + 8),
            2,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx + 12),
            cx_i32(cy + 12),
            cx_i32(cx + 8),
            cx_i32(cy + 8),
            2,
            color,
        );
    }

    fn draw_shield(&self, canvas: &mut Canvas<'_>, cx: usize, cy: usize, size: usize, color: u32) {
        let half = size / 2;
        let stroke = if size >= 24 { 3 } else { 2 };
        canvas.stroke_line_i32(
            cx_i32(cx),
            cx_i32(cy) - cx_i32(size),
            cx_i32(cx) + cx_i32(half),
            cx_i32(cy) - cx_i32(half),
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx) + cx_i32(half),
            cx_i32(cy) - cx_i32(half),
            cx_i32(cx) + cx_i32(half),
            cx_i32(cy) + cx_i32(half),
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx) + cx_i32(half),
            cx_i32(cy) + cx_i32(half),
            cx_i32(cx),
            cx_i32(cy) + cx_i32(size),
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx),
            cx_i32(cy) + cx_i32(size),
            cx_i32(cx) - cx_i32(half),
            cx_i32(cy) + cx_i32(half),
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx) - cx_i32(half),
            cx_i32(cy) + cx_i32(half),
            cx_i32(cx) - cx_i32(half),
            cx_i32(cy) - cx_i32(half),
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx) - cx_i32(half),
            cx_i32(cy) - cx_i32(half),
            cx_i32(cx),
            cx_i32(cy) - cx_i32(size),
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx) - 6,
            cx_i32(cy),
            cx_i32(cx) - 1,
            cx_i32(cy) + 6,
            stroke,
            color,
        );
        canvas.stroke_line_i32(
            cx_i32(cx) - 1,
            cx_i32(cy) + 6,
            cx_i32(cx) + 9,
            cx_i32(cy) - 7,
            stroke,
            color,
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

impl Frame {
    pub fn new(width: usize, height: usize, color: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; width.saturating_mul(height)],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            let index = y.saturating_mul(self.width).saturating_add(x);
            if let Some(pixel) = self.pixels.get_mut(index) {
                *pixel = color;
            }
        }
    }

    fn blend_pixel_i32(&mut self, x: i32, y: i32, color: u32, alpha: u8) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };

        if x >= self.width || y >= self.height {
            return;
        }

        let index = y.saturating_mul(self.width).saturating_add(x);
        let Some(pixel) = self.pixels.get_mut(index) else {
            return;
        };

        let alpha = u32::from(alpha);
        let inverse_alpha = 255_u32.saturating_sub(alpha);
        let red = blend_channel(
            channel(color, 16),
            channel(*pixel, 16),
            alpha,
            inverse_alpha,
        );
        let green = blend_channel(channel(color, 8), channel(*pixel, 8), alpha, inverse_alpha);
        let blue = blend_channel(channel(color, 0), channel(*pixel, 0), alpha, inverse_alpha);
        *pixel = (red << 16) | (green << 8) | blue;
    }
}

struct Canvas<'a> {
    frame: &'a mut Frame,
    text: &'a TextRenderer,
}

impl<'a> Canvas<'a> {
    fn new(frame: &'a mut Frame, text: &'a TextRenderer) -> Self {
        Self { frame, text }
    }

    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let end_x = x.saturating_add(w).min(self.frame.width);
        let end_y = y.saturating_add(h).min(self.frame.height);
        for py in y.min(self.frame.height)..end_y {
            for px in x.min(self.frame.width)..end_x {
                self.frame.set_pixel(px, py, color);
            }
        }
    }

    fn rounded_rect(&mut self, x: usize, y: usize, w: usize, h: usize, radius: usize, color: u32) {
        if w == 0 || h == 0 {
            return;
        }

        let radius = radius.min(w / 2).min(h / 2);
        if radius == 0 {
            self.rect(x, y, w, h, color);
            return;
        }

        let end_x = x.saturating_add(w).min(self.frame.width);
        let end_y = y.saturating_add(h).min(self.frame.height);
        let left_center = x.saturating_add(radius);
        let right_center = x.saturating_add(w.saturating_sub(radius + 1));
        let top_center = y.saturating_add(radius);
        let bottom_center = y.saturating_add(h.saturating_sub(radius + 1));
        let radius_i32 = cx_i32(radius);
        let radius_sq = radius_i32.saturating_mul(radius_i32);

        for py in y.min(self.frame.height)..end_y {
            for px in x.min(self.frame.width)..end_x {
                let center_x = if px < left_center {
                    left_center
                } else if px > right_center {
                    right_center
                } else {
                    px
                };
                let center_y = if py < top_center {
                    top_center
                } else if py > bottom_center {
                    bottom_center
                } else {
                    py
                };
                let dx = cx_i32(px).saturating_sub(cx_i32(center_x));
                let dy = cx_i32(py).saturating_sub(cx_i32(center_y));
                let distance = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
                if distance <= radius_sq {
                    self.frame.set_pixel(px, py, color);
                }
            }
        }
    }

    fn rounded_rect_panel(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        radius: usize,
        border: u32,
        fill: u32,
    ) {
        self.rounded_rect(x, y, w, h, radius, border);
        if w > 2 && h > 2 {
            self.rounded_rect(
                x + 1,
                y + 1,
                w.saturating_sub(2),
                h.saturating_sub(2),
                radius.saturating_sub(1),
                fill,
            );
        }
    }

    fn rect_border(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.hline(x, y, w, color);
        self.hline(x, y.saturating_add(h.saturating_sub(1)), w, color);
        self.vline(x, y, h, color);
        self.vline(x.saturating_add(w.saturating_sub(1)), y, h, color);
    }

    fn hline(&mut self, x: usize, y: usize, w: usize, color: u32) {
        for px in x.min(self.frame.width)..x.saturating_add(w).min(self.frame.width) {
            self.frame.set_pixel(px, y, color);
        }
    }

    fn vline(&mut self, x: usize, y: usize, h: usize, color: u32) {
        for py in y.min(self.frame.height)..y.saturating_add(h).min(self.frame.height) {
            self.frame.set_pixel(x, py, color);
        }
    }

    fn draw_text(&mut self, x: usize, y: usize, scale: usize, text: &str, color: u32) {
        let scale = scale.max(1);
        if self
            .text
            .draw_text(self.frame, x, y, text_size(scale), text, color, None)
        {
            return;
        }

        let mut cursor = x;
        for ch in text.chars() {
            cursor = cursor.saturating_add(self.draw_char(cursor, y, scale, ch, color));
        }
    }

    fn draw_text_clipped(
        &mut self,
        x: usize,
        y: usize,
        scale: usize,
        text: &str,
        color: u32,
        max_width: usize,
    ) {
        let scale = scale.max(1);
        if self.text.draw_text(
            self.frame,
            x,
            y,
            text_size(scale),
            text,
            color,
            Some(max_width),
        ) {
            return;
        }

        let char_w = 6 * scale;
        let max_chars = max_width / char_w;
        if max_chars == 0 {
            return;
        }

        let mut cursor = x;
        for ch in text.chars().take(max_chars) {
            cursor = cursor.saturating_add(self.draw_char(cursor, y, scale, ch, color));
        }
    }

    fn draw_text_centered(
        &mut self,
        x: usize,
        y: usize,
        scale: usize,
        text: &str,
        color: u32,
        max_width: usize,
    ) {
        let scale = scale.max(1);
        let text_width = self
            .text
            .measure_width(text_size(scale), text)
            .unwrap_or_else(|| text.chars().count().saturating_mul(6 * scale))
            .min(max_width);
        let offset = max_width.saturating_sub(text_width) / 2;
        self.draw_text_clipped(x + offset, y, scale, text, color, max_width);
    }

    fn measure_text(&self, scale: usize, text: &str) -> usize {
        let scale = scale.max(1);
        self.text
            .measure_width(text_size(scale), text)
            .unwrap_or_else(|| text.chars().count().saturating_mul(6 * scale))
    }

    fn draw_alpha_mask_centered(
        &mut self,
        cx: usize,
        cy: usize,
        width: usize,
        height: usize,
        mask: &[u8],
        color: u32,
    ) {
        let start_x = cx.saturating_sub(width / 2);
        let start_y = cy.saturating_sub(height / 2);

        for row in 0..height {
            for col in 0..width {
                let index = row.saturating_mul(width).saturating_add(col);
                let Some(alpha) = mask.get(index).copied() else {
                    continue;
                };
                if alpha == 0 {
                    continue;
                }

                self.frame.blend_pixel_i32(
                    cx_i32(start_x.saturating_add(col)),
                    cx_i32(start_y.saturating_add(row)),
                    color,
                    alpha,
                );
            }
        }
    }

    fn draw_char(&mut self, x: usize, y: usize, scale: usize, ch: char, color: u32) -> usize {
        let glyph = glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                let mask = 1_u8 << (4 - col);
                if bits & mask != 0 {
                    self.rect(
                        x.saturating_add(col * scale),
                        y.saturating_add(row * scale),
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        6 * scale
    }

    fn circle_fill(&mut self, cx: usize, cy: usize, radius: usize, color: u32) {
        let cx = cx_i32(cx);
        let cy = cx_i32(cy);
        let radius = cx_i32(radius);
        let outer = radius.saturating_mul(radius);

        for y in cy.saturating_sub(radius)..=cy.saturating_add(radius) {
            for x in cx.saturating_sub(radius)..=cx.saturating_add(radius) {
                let dx = x.saturating_sub(cx);
                let dy = y.saturating_sub(cy);
                let distance = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
                if distance <= outer {
                    self.set_pixel_i32(x, y, color);
                }
            }
        }
    }

    fn line_i32(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            self.set_pixel_i32(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let double_error = error.saturating_mul(2);
            if double_error >= dy {
                error += dy;
                x += sx;
            }
            if double_error <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    fn stroke_line_i32(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, width: usize, color: u32) {
        let padding = cx_i32(width).saturating_add(2);
        let max_frame_x = cx_i32(self.frame.width.saturating_sub(1));
        let max_frame_y = cx_i32(self.frame.height.saturating_sub(1));
        let min_x = x0.min(x1).saturating_sub(padding).max(0);
        let max_x = x0.max(x1).saturating_add(padding).min(max_frame_x);
        let min_y = y0.min(y1).saturating_sub(padding).max(0);
        let max_y = y0.max(y1).saturating_add(padding).min(max_frame_y);

        let x0f = f64::from(x0);
        let y0f = f64::from(y0);
        let x1f = f64::from(x1);
        let y1f = f64::from(y1);
        let dx = x1f - x0f;
        let dy = y1f - y0f;
        let length_sq = dx * dx + dy * dy;
        let half_width = f64::from(cx_i32(width)) / 2.0;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let point_x = f64::from(px) + 0.5;
                let point_y = f64::from(py) + 0.5;
                let distance = if length_sq <= 0.0 {
                    distance(point_x, point_y, x0f, y0f)
                } else {
                    let t =
                        (((point_x - x0f) * dx + (point_y - y0f) * dy) / length_sq).clamp(0.0, 1.0);
                    let nearest_x = x0f + t * dx;
                    let nearest_y = y0f + t * dy;
                    distance(point_x, point_y, nearest_x, nearest_y)
                };
                let coverage = half_width + 0.75 - distance;
                if coverage > 0.0 {
                    self.frame
                        .blend_pixel_i32(px, py, color, coverage_to_alpha(coverage));
                }
            }
        }
    }

    fn stroke_circle_i32(&mut self, cx: i32, cy: i32, radius: usize, width: usize, color: u32) {
        let padding = cx_i32(radius.saturating_add(width).saturating_add(2));
        let max_frame_x = cx_i32(self.frame.width.saturating_sub(1));
        let max_frame_y = cx_i32(self.frame.height.saturating_sub(1));
        let min_x = cx.saturating_sub(padding).max(0);
        let max_x = cx.saturating_add(padding).min(max_frame_x);
        let min_y = cy.saturating_sub(padding).max(0);
        let max_y = cy.saturating_add(padding).min(max_frame_y);
        let radius = f64::from(cx_i32(radius));
        let half_width = f64::from(cx_i32(width)) / 2.0;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let point_x = f64::from(px) + 0.5;
                let point_y = f64::from(py) + 0.5;
                let edge_distance =
                    (distance(point_x, point_y, f64::from(cx), f64::from(cy)) - radius).abs();
                let coverage = half_width + 0.75 - edge_distance;
                if coverage > 0.0 {
                    self.frame
                        .blend_pixel_i32(px, py, color, coverage_to_alpha(coverage));
                }
            }
        }
    }

    fn set_pixel_i32(&mut self, x: i32, y: i32, color: u32) {
        if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
            self.frame.set_pixel(x, y, color);
        }
    }
}

fn cx_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn usize_to_f32(value: usize) -> f32 {
    value.to_string().parse().unwrap_or(0.0)
}

fn f32_to_i32_ceil(value: f32) -> i32 {
    if value.is_finite() {
        format!("{:.0}", value.ceil()).parse().unwrap_or(0)
    } else {
        0
    }
}

fn f32_to_i32_floor(value: f32) -> i32 {
    if value.is_finite() {
        format!("{:.0}", value.floor()).parse().unwrap_or(0)
    } else {
        0
    }
}

fn f32_to_usize_ceil(value: f32) -> usize {
    if value.is_finite() && value > 0.0 {
        format!("{:.0}", value.ceil()).parse().unwrap_or(0)
    } else {
        0
    }
}

fn whitespace_advance(size: usize) -> usize {
    (size / 3).max(4)
}

fn distance(x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    (dx * dx + dy * dy).sqrt()
}

fn coverage_to_alpha(coverage: f64) -> u8 {
    if coverage.is_finite() {
        format!("{:.0}", coverage.clamp(0.0, 1.0) * 255.0)
            .parse()
            .unwrap_or(0)
    } else {
        0
    }
}

fn text_size(scale: usize) -> usize {
    match scale {
        0 | 1 => 13,
        2 => 16,
        3 => 24,
        other => other.saturating_mul(8),
    }
}

fn metric_color(accent: MetricAccent) -> u32 {
    match accent {
        MetricAccent::Teal => TEAL,
        MetricAccent::Amber => AMBER,
        MetricAccent::Blue => BLUE,
    }
}

fn app_icon_mask(icon: AppIcon) -> &'static [u8] {
    match icon {
        AppIcon::Globe => WEB_ICON_MASK,
        AppIcon::Download => DOWNLOADS_ICON_MASK,
        AppIcon::Calendar => CALENDAR_ICON_MASK,
        AppIcon::Message => MESSAGING_ICON_MASK,
    }
}

fn tab_icon_mask(index: usize) -> &'static [u8] {
    match index {
        0 => TAB_WEB_ICON_MASK,
        1 => TAB_RESEARCH_ICON_MASK,
        _ => TAB_CALENDAR_ICON_MASK,
    }
}

fn home_metric_icon_mask(index: usize) -> &'static [u8] {
    match index {
        0 => HOME_METRIC_PRIVACY_MASK,
        1 => HOME_METRIC_LOCK_MASK,
        2 => HOME_METRIC_ADS_MASK,
        _ => HOME_METRIC_TIME_MASK,
    }
}

fn window_control_positions(width: usize) -> [usize; 3] {
    let total_width = WINDOW_CONTROL_W
        .saturating_mul(3)
        .saturating_add(WINDOW_CONTROL_GAP.saturating_mul(2));
    let start = width.saturating_sub(total_width.saturating_add(18));
    [
        start,
        start.saturating_add(WINDOW_CONTROL_W + WINDOW_CONTROL_GAP),
        start.saturating_add((WINDOW_CONTROL_W + WINDOW_CONTROL_GAP).saturating_mul(2)),
    ]
}

fn channel(color: u32, shift: u32) -> u32 {
    (color >> shift) & 0xff
}

fn blend_channel(source: u32, destination: u32, alpha: u32, inverse_alpha: u32) -> u32 {
    source
        .saturating_mul(alpha)
        .saturating_add(destination.saturating_mul(inverse_alpha))
        / 255
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '/' => [
            0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '+' => [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
        '<' => [
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ],
        '>' => [
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ],
        '*' => [
            0b00100, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0b00100,
        ],
        '=' => [
            0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        ' ' => [0, 0, 0, 0, 0, 0, 0],
        _ => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        APP_ICON_SIZE, ChromeView, HOME_FOOTER_SHIELD_MASK, HOME_FOOTER_SHIELD_SIZE,
        HOME_HERO_SHIELD_MASK, HOME_HERO_SHIELD_SIZE, HOME_METRIC_ICON_SIZE, HOME_SEARCH_ICON_MASK,
        HOME_SEARCH_ICON_SIZE, NAV_BACK_ICON_MASK, NAV_FORWARD_ICON_MASK, NAV_ICON_SIZE,
        NAV_REFRESH_ICON_MASK, TAB_H, TAB_ICON_SIZE, TAB_X, TAB_Y, TOP_SHIELD_ICON_MASK,
        TOP_SHIELD_ICON_SIZE, WindowCommand, app_icon_mask, home_metric_icon_mask, tab_icon_mask,
        window_control_positions,
    };
    use slate_apps::{AppIcon, AppId};
    use slate_browser_core::BrowserState;
    use slate_rendering::ServoBackend;

    #[test]
    fn renders_non_empty_frame() {
        let state = BrowserState::new(&ServoBackend);
        let frame = ChromeView::new(state).render(800, 500);
        assert_eq!(frame.width(), 800);
        assert_eq!(frame.height(), 500);
        assert!(frame.pixels().iter().any(|pixel| *pixel != 0x00FAF9F6));
    }

    #[test]
    fn side_rail_click_selects_app() {
        let state = BrowserState::new(&ServoBackend);
        let mut view = ChromeView::new(state);
        view.handle_click(40, TAB_H + 22 + 72 + 20, 1280, 720);
        assert_eq!(view.state().active_app, AppId::Downloads);
    }

    #[test]
    fn new_tab_click_adds_tab() {
        let state = BrowserState::new(&ServoBackend);
        let mut view = ChromeView::new(state);
        let tab_w = view.tab_width(1280);
        let plus_x = TAB_X + view.visible_tab_count() * (tab_w + 2) + 20;
        view.handle_click(plus_x, TAB_Y + 20, 1280, 720);
        assert_eq!(view.state().tabs.len(), 4);
        assert_eq!(view.state().active_tab, 3);
    }

    #[test]
    fn window_control_clicks_return_commands() {
        let state = BrowserState::new(&ServoBackend);
        let mut view = ChromeView::new(state);
        let [minimize_x, maximize_x, close_x] = window_control_positions(1280);

        assert_eq!(
            view.handle_click(minimize_x + 12, TAB_Y + 12, 1280, 720),
            WindowCommand::Minimize
        );
        assert_eq!(
            view.handle_click(maximize_x + 12, TAB_Y + 12, 1280, 720),
            WindowCommand::ToggleMaximize
        );
        assert_eq!(
            view.handle_click(close_x + 12, TAB_Y + 12, 1280, 720),
            WindowCommand::Close
        );
    }

    #[test]
    fn top_chrome_has_draggable_regions() {
        let state = BrowserState::new(&ServoBackend);
        let view = ChromeView::new(state);
        let [_, _, close_x] = window_control_positions(1280);
        let tab_w = view.tab_width(1280);
        let plus_x = TAB_X + view.visible_tab_count() * (tab_w + 2) + 20;

        assert!(view.is_draggable_chrome(32, TAB_Y + 18, 1280, 720));
        assert!(!view.is_draggable_chrome(TAB_X + 20, TAB_Y + 18, 1280, 720));
        assert!(!view.is_draggable_chrome(close_x + 12, TAB_Y + 12, 1280, 720));
        assert!(!view.is_draggable_chrome(plus_x, TAB_Y + 20, 1280, 720));
        assert!(!view.is_draggable_chrome(220, TAB_H + 20, 1280, 720));
    }

    #[test]
    fn app_icon_masks_match_declared_size() {
        for icon in [
            AppIcon::Globe,
            AppIcon::Download,
            AppIcon::Calendar,
            AppIcon::Message,
        ] {
            assert_eq!(app_icon_mask(icon).len(), APP_ICON_SIZE * APP_ICON_SIZE);
        }
    }

    #[test]
    fn tab_icon_masks_match_declared_size() {
        for index in 0..3 {
            assert_eq!(tab_icon_mask(index).len(), TAB_ICON_SIZE * TAB_ICON_SIZE);
        }
    }

    #[test]
    fn toolbar_icon_masks_match_declared_sizes() {
        for mask in [
            NAV_BACK_ICON_MASK,
            NAV_FORWARD_ICON_MASK,
            NAV_REFRESH_ICON_MASK,
        ] {
            assert_eq!(mask.len(), NAV_ICON_SIZE * NAV_ICON_SIZE);
        }

        assert_eq!(
            TOP_SHIELD_ICON_MASK.len(),
            TOP_SHIELD_ICON_SIZE * TOP_SHIELD_ICON_SIZE
        );
    }

    #[test]
    fn home_icon_masks_match_declared_sizes() {
        assert_eq!(
            HOME_HERO_SHIELD_MASK.len(),
            HOME_HERO_SHIELD_SIZE * HOME_HERO_SHIELD_SIZE
        );
        assert_eq!(
            HOME_SEARCH_ICON_MASK.len(),
            HOME_SEARCH_ICON_SIZE * HOME_SEARCH_ICON_SIZE
        );
        assert_eq!(
            HOME_FOOTER_SHIELD_MASK.len(),
            HOME_FOOTER_SHIELD_SIZE * HOME_FOOTER_SHIELD_SIZE
        );
        for index in 0..4 {
            assert_eq!(
                home_metric_icon_mask(index).len(),
                HOME_METRIC_ICON_SIZE * HOME_METRIC_ICON_SIZE
            );
        }
    }
}
