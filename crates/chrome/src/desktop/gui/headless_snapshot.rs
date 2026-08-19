/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use egui::epaint::{ImageData, Primitive, Vertex};
use egui::{ClippedPrimitive, Color32, TextureId};
use image::RgbaImage;
use servo::LoadStatus;
use slate_broadwebd::BroadwebStatusSnapshot;

use super::{
    ADDRESS_BOOKMARK_BUTTON_SIZE, ADDRESS_BOOKMARK_ICON_SIZE, ADDRESS_BOOKMARK_RESERVED_WIDTH,
    ADDRESS_CORNER_RADIUS, ADDRESS_HEIGHT, ADDRESS_ICON_GAP, ADDRESS_INNER_MARGIN_X,
    ADDRESS_INPUT_TEXT_SIZE, ADDRESS_LEADING_GAP, ADDRESS_SECURITY_ICON_SIZE, ADDRESS_TEXT_HEIGHT,
    ADDRESS_TRAILING_GAP, APP_RAIL_MAX_WIDTH, APP_RAIL_WIDTH, AddressSecurityIcon, FOOTER_HEIGHT,
    Gui, RAIL_BUTTON_SIZE, RAIL_ITEM_GAP, RAIL_PANEL_MARGIN_X, RAIL_PANEL_MARGIN_Y, RAIL_TOP_SPACE,
    RailDownloadTabPreview, RailPage, RailWebTabPreview, TOOLBAR_BUTTON_SIZE, TOOLBAR_HEIGHT,
    TOOLBAR_ICON_SIZE, TOOLBAR_ITEM_SPACING, TOOLBAR_PANEL_MARGIN_X, TOOLBAR_PANEL_MARGIN_Y,
    TOOLBAR_SEPARATOR_HEIGHT, TOOLBAR_SEPARATOR_LEADING_GAP, TOOLBAR_SEPARATOR_TRAILING_GAP,
    address_background_color, address_bookmark_icon_color, address_border_color,
    address_security_icon_for_location, address_security_raster_color,
    address_slate_security_icon_rect, chrome_panel_background_color, configure_fonts,
    default_home_bookmark_cards, footer_panel_margin, home_view_background_color,
    rail_button_width, rail_collapsed_tab_line_rect, rail_icon_slot_rect, rail_item_height,
    rail_tab_close_button_rect, rail_tab_row_rect, rail_web_item_height, slate_theme,
    tab_icon_color, toolbar_address_width, toolbar_background_color, toolbar_navigation_icon_rect,
};
use crate::desktop::slate_theme::{SlateIcon, SlateIconCache, SlateRaster};

pub(crate) const DEFAULT_SNAPSHOT_WIDTH: u32 = 1672;
pub(crate) const DEFAULT_SNAPSHOT_HEIGHT: u32 = 941;

pub(crate) fn write_default_snapshot(path: &Path) -> Result<(), String> {
    write_snapshot(path, [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT])
}

pub(crate) fn write_snapshot(path: &Path, size: [u32; 2]) -> Result<(), String> {
    let image = render_snapshot(size)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
    }
    image
        .save(path)
        .map_err(|error| format!("failed to encode PNG: {error}"))
}

pub(crate) fn write_default_verification_report(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;

    let image = render_snapshot([DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT])?;
    let full_name = "full.png";
    let full_path = directory.join(full_name);
    image
        .save(&full_path)
        .map_err(|error| format!("failed to encode {}: {error}", full_path.display()))?;
    let loading_image = render_snapshot_with_load_status(
        [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT],
        LoadStatus::Started,
    )?;
    let loading_full_name = "loading-full.png";
    let loading_full_path = directory.join(loading_full_name);
    loading_image
        .save(&loading_full_path)
        .map_err(|error| format!("failed to encode {}: {error}", loading_full_path.display()))?;
    let chrome = snapshot_chrome_geometry();
    let toolbar = snapshot_toolbar_controls_geometry(chrome);
    let hover_back_image = render_snapshot_with_interaction(
        [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT],
        LoadStatus::Complete,
        Some(toolbar.nav_button_rects[0].center()),
        true,
    )?;
    let hover_back_full_name = "hover-nav-back-full.png";
    let hover_back_full_path = directory.join(hover_back_full_name);
    hover_back_image
        .save(&hover_back_full_path)
        .map_err(|error| {
            format!(
                "failed to encode {}: {error}",
                hover_back_full_path.display()
            )
        })?;
    let hover_reload_image = render_snapshot_with_interaction(
        [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT],
        LoadStatus::Complete,
        Some(toolbar.nav_button_rects[2].center()),
        false,
    )?;
    let hover_reload_full_name = "hover-nav-reload-full.png";
    let hover_reload_full_path = directory.join(hover_reload_full_name);
    hover_reload_image
        .save(&hover_reload_full_path)
        .map_err(|error| {
            format!(
                "failed to encode {}: {error}",
                hover_reload_full_path.display()
            )
        })?;
    let hover_menu_image = render_snapshot_with_interaction(
        [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT],
        LoadStatus::Complete,
        Some(toolbar.menu_button_rect.center()),
        false,
    )?;
    let hover_menu_full_name = "hover-menu-full.png";
    let hover_menu_full_path = directory.join(hover_menu_full_name);
    hover_menu_image
        .save(&hover_menu_full_path)
        .map_err(|error| {
            format!(
                "failed to encode {}: {error}",
                hover_menu_full_path.display()
            )
        })?;
    let expanded_rail_image = render_snapshot_with_rail_width(
        [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT],
        APP_RAIL_MAX_WIDTH,
    )?;
    let expanded_rail_full_name = "expanded-rail-full.png";
    let expanded_rail_full_path = directory.join(expanded_rail_full_name);
    expanded_rail_image
        .save(&expanded_rail_full_path)
        .map_err(|error| {
            format!(
                "failed to encode {}: {error}",
                expanded_rail_full_path.display()
            )
        })?;

    let mut summary = VerificationSummary::default();
    let mut region_captures = HashMap::new();
    let mut region_reports = Vec::new();
    for region in verification_regions() {
        let source_image = match region.source {
            VerificationSource::Complete => &image,
            VerificationSource::Loading => &loading_image,
            VerificationSource::HoverNavBack => &hover_back_image,
            VerificationSource::HoverNavReload => &hover_reload_image,
            VerificationSource::HoverMenu => &hover_menu_image,
            VerificationSource::ExpandedRail => &expanded_rail_image,
        };
        let crop_rect = PixelRect::from_point_rect(
            region.rect,
            1.0,
            source_image.width(),
            source_image.height(),
        );
        let crop = image::imageops::crop_imm(
            source_image,
            crop_rect.min_x,
            crop_rect.min_y,
            crop_rect.width(),
            crop_rect.height(),
        )
        .to_image();
        let crop_path = directory.join(region.file_name);
        crop.save(&crop_path)
            .map_err(|error| format!("failed to encode {}: {error}", crop_path.display()))?;
        let metrics = crop_metrics(&crop);
        let monitor = evaluate_region_monitor(region.monitor, crop_rect, metrics);
        summary.record(&monitor, region.monitor);
        region_captures.insert(
            region.name,
            RegionCapture {
                rect: crop_rect,
                metrics,
            },
        );

        region_reports.push(serde_json::json!({
            "name": region.name,
            "file": region.file_name,
            "source": region.source.as_str(),
            "purpose": region.purpose,
            "rect": pixel_rect_json(crop_rect),
            "metrics": crop_metrics_json(metrics),
            "monitor": region_monitor_json(region.monitor, &monitor),
        }));
    }

    let report = serde_json::json!({
        "schema": "slate.chrome.visual-verification.v1",
        "viewport": {
            "width": DEFAULT_SNAPSHOT_WIDTH,
            "height": DEFAULT_SNAPSHOT_HEIGHT,
            "pixels_per_point": 1.0,
        },
        "full": full_name,
        "state_images": {
            "complete": full_name,
            "loading": loading_full_name,
            "hover_nav_back": hover_back_full_name,
            "hover_nav_reload": hover_reload_full_name,
            "hover_menu": hover_menu_full_name,
            "expanded_rail": expanded_rail_full_name,
        },
        "summary": verification_summary_json(summary),
        "regions": region_reports,
        "automated_review": automated_review_json(&region_captures),
        "checks": {
            "manual_review_required": true,
            "review_focus": [
                "compact left rail tab rows stay grouped under their owning app icons and expose close affordances",
                "default-width rail rendering remains stable while production rail resizing can reveal wider tab titles",
                "previously fixed Home, tab icon, tab clipping, divider, and close artwork issues stay covered by stable crops",
                "Reload and loading-state Stop artwork are rendered from separate canonical states",
                "manual review covers theme consistency and alignment qualities that threshold metrics cannot fully judge"
            ],
            "metrics": [
                "detail_pixel bounds track glyph presence and gross alignment",
                "dark_pixel counts catch missing or washed-out raster/vector glyphs",
                "vertical_detail_columns highlight unexpected divider-like artifacts"
            ]
        }
    });
    let report_path = directory.join("report.json");
    let report_json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("failed to serialize verification report: {error}"))?;
    fs::write(&report_path, report_json)
        .map_err(|error| format!("failed to write {}: {error}", report_path.display()))
}

fn render_snapshot(size: [u32; 2]) -> Result<RgbaImage, String> {
    render_snapshot_with_load_status(size, LoadStatus::Complete)
}

fn render_snapshot_with_rail_width(size: [u32; 2], rail_width: f32) -> Result<RgbaImage, String> {
    render_snapshot_with_interaction_and_rail_width(
        size,
        LoadStatus::Complete,
        None,
        false,
        rail_width,
    )
}

fn render_snapshot_with_load_status(
    size: [u32; 2],
    load_status: LoadStatus,
) -> Result<RgbaImage, String> {
    render_snapshot_with_interaction(size, load_status, None, false)
}

fn render_snapshot_with_interaction(
    size: [u32; 2],
    load_status: LoadStatus,
    hover_pos: Option<egui::Pos2>,
    can_go_back: bool,
) -> Result<RgbaImage, String> {
    render_snapshot_with_interaction_and_rail_width(
        size,
        load_status,
        hover_pos,
        can_go_back,
        APP_RAIL_WIDTH,
    )
}

fn render_snapshot_with_interaction_and_rail_width(
    size: [u32; 2],
    load_status: LoadStatus,
    hover_pos: Option<egui::Pos2>,
    can_go_back: bool,
    rail_width: f32,
) -> Result<RgbaImage, String> {
    let ctx = egui::Context::default();
    ctx.set_fonts(configure_fonts());
    ctx.options_mut(|options| {
        options.zoom_with_keyboard = false;
        options.fallback_theme = egui::Theme::Light;
    });
    slate_theme::apply(&ctx);

    let mut slate_icons = SlateIconCache::default();
    let mut location = "slate://home".to_owned();
    let mut home_search = String::new();
    let mut renderer = SoftwareRenderer::new(size);
    if hover_pos.is_some() {
        let warmup_output = render_snapshot_frame(
            &ctx,
            size,
            0.0,
            hover_pos,
            load_status,
            can_go_back,
            &mut slate_icons,
            &mut location,
            &mut home_search,
            rail_width,
        );
        renderer.apply_textures_delta(&warmup_output.textures_delta)?;
    }
    let output = render_snapshot_frame(
        &ctx,
        size,
        1.0 / 60.0,
        hover_pos,
        load_status,
        can_go_back,
        &mut slate_icons,
        &mut location,
        &mut home_search,
        rail_width,
    );
    renderer.apply_textures_delta(&output.textures_delta)?;
    let pixels_per_point = output.pixels_per_point;
    let clipped_primitives = ctx.tessellate(output.shapes, pixels_per_point);
    renderer.paint(&clipped_primitives, pixels_per_point)?;
    Ok(renderer.into_image())
}

fn render_snapshot_frame(
    ctx: &egui::Context,
    size: [u32; 2],
    time: f64,
    hover_pos: Option<egui::Pos2>,
    load_status: LoadStatus,
    can_go_back: bool,
    slate_icons: &mut SlateIconCache,
    location: &mut String,
    home_search: &mut String,
    rail_width: f32,
) -> egui::FullOutput {
    let screen_rect =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32));
    let mut input = egui::RawInput {
        screen_rect: Some(screen_rect),
        focused: true,
        time: Some(time),
        ..Default::default()
    };
    if let Some(hover_pos) = hover_pos {
        input.events.push(egui::Event::PointerMoved(hover_pos));
    }
    if let Some(viewport) = input.viewports.get_mut(&egui::ViewportId::ROOT) {
        viewport.native_pixels_per_point = Some(1.0);
        viewport.inner_rect = Some(screen_rect);
    }

    ctx.run_ui(input, |ui| {
        render_chrome_fixture(
            ui,
            slate_icons,
            location,
            home_search,
            load_status,
            can_go_back,
            rail_width,
        );
    })
}

fn render_chrome_fixture(
    root_ui: &mut egui::Ui,
    slate_icons: &mut SlateIconCache,
    location: &mut String,
    home_search: &mut String,
    load_status: LoadStatus,
    can_go_back: bool,
    rail_width: f32,
) {
    render_app_rail(root_ui, slate_icons, rail_width);
    render_toolbar(root_ui, slate_icons, location, load_status, can_go_back);
    let footer_rect = render_footer(root_ui, load_status);

    render_home_panel(root_ui, slate_icons, home_search);
    Gui::draw_footer_top_separator(root_ui.ctx(), footer_rect);
}

fn render_app_rail(root_ui: &mut egui::Ui, slate_icons: &mut SlateIconCache, rail_width: f32) {
    let rail_frame = egui::Frame::NONE
        .fill(chrome_panel_background_color())
        .inner_margin(egui::Margin::symmetric(
            RAIL_PANEL_MARGIN_X,
            RAIL_PANEL_MARGIN_Y,
        ));
    egui::Panel::left("headless_app_rail")
        .exact_size(rail_width)
        .frame(rail_frame)
        .show_separator_line(true)
        .show_inside(root_ui, |ui| {
            let web_tabs = snapshot_rail_web_tab_previews(ui, slate_icons);
            let download_tabs = snapshot_rail_download_tab_previews();
            let _ = Gui::draw_app_rail(
                ui,
                slate_icons,
                Some(RailPage::Web),
                &web_tabs,
                &download_tabs,
            );
        });
}

fn snapshot_rail_web_tab_previews(
    ui: &egui::Ui,
    slate_icons: &mut SlateIconCache,
) -> Vec<RailWebTabPreview> {
    [
        ("New Tab", true),
        ("Privacy Dashboard", false),
        ("Calendar", false),
    ]
    .into_iter()
    .map(|(label, active)| RailWebTabPreview {
        webview_id: None,
        label: label.to_string(),
        icon: slate_icons.texture(
            ui.ctx(),
            Gui::fallback_tab_icon_for_page(Some(label), None),
            tab_icon_color(active),
        ),
        active,
    })
    .collect()
}

fn snapshot_rail_download_tab_previews() -> Vec<RailDownloadTabPreview> {
    vec![
        RailDownloadTabPreview {
            label: "servo-book.pdf".to_string(),
            progress: Some(0.42),
        },
        RailDownloadTabPreview {
            label: "site-archive.car".to_string(),
            progress: Some(1.0),
        },
    ]
}

fn render_toolbar(
    root_ui: &mut egui::Ui,
    slate_icons: &mut SlateIconCache,
    location: &mut String,
    load_status: LoadStatus,
    can_go_back: bool,
) {
    let toolbar_frame = egui::Frame::NONE
        .fill(toolbar_background_color())
        .inner_margin(egui::Margin::symmetric(
            TOOLBAR_PANEL_MARGIN_X,
            TOOLBAR_PANEL_MARGIN_Y,
        ));
    egui::Panel::top("headless_toolbar")
        .exact_size(TOOLBAR_HEIGHT)
        .frame(toolbar_frame)
        .show_separator_line(true)
        .show_inside(root_ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(TOOLBAR_ITEM_SPACING, 0.0);
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let _ = Gui::toolbar_navigation_button(
                        ui,
                        slate_icons,
                        SlateIcon::NavBack,
                        can_go_back,
                    );
                    let _ = Gui::toolbar_navigation_button(
                        ui,
                        slate_icons,
                        SlateIcon::NavForward,
                        false,
                    );
                    match load_status {
                        LoadStatus::Started | LoadStatus::HeadParsed => {
                            let _ = Gui::toolbar_stop_button(ui, slate_icons, true);
                        }
                        LoadStatus::Complete => {
                            let _ = Gui::toolbar_navigation_button(
                                ui,
                                slate_icons,
                                SlateIcon::NavRefresh,
                                true,
                            );
                        }
                    }

                    ui.add_space(ADDRESS_LEADING_GAP);
                    draw_snapshot_address_field(ui, slate_icons, location);
                    ui.add_space(ADDRESS_TRAILING_GAP);

                    let privacy_icon =
                        slate_icons.texture(ui.ctx(), SlateIcon::TopShield, slate_theme::AMBER);
                    let _ = Gui::toolbar_icon_button_sized(
                        ui,
                        privacy_icon,
                        super::TOOLBAR_PRIVACY_ICON_SIZE,
                    );

                    let toolbar_item_spacing = ui.spacing().item_spacing.x;
                    ui.spacing_mut().item_spacing.x = super::TOOLBAR_SEPARATOR_LEADING_GAP;
                    Gui::vertical_separator(ui, super::TOOLBAR_SEPARATOR_HEIGHT);

                    ui.spacing_mut().item_spacing.x = super::TOOLBAR_SEPARATOR_TRAILING_GAP;
                    let _ = Gui::toolbar_menu_button(ui, false);
                    ui.spacing_mut().item_spacing.x = toolbar_item_spacing;
                },
            );
        });
}

fn draw_snapshot_address_field(
    ui: &mut egui::Ui,
    slate_icons: &mut SlateIconCache,
    location: &mut String,
) {
    let available_for_address = ui.available_width().max(0.0);
    let address_width = toolbar_address_width(available_for_address);
    egui::Frame::NONE
        .fill(address_background_color())
        .stroke(egui::Stroke::new(1.0_f32, address_border_color()))
        .corner_radius(ADDRESS_CORNER_RADIUS)
        .shadow(super::address_shadow())
        .inner_margin(egui::Margin::symmetric(ADDRESS_INNER_MARGIN_X, 0))
        .show(ui, |ui| {
            ui.set_width(address_width);
            ui.set_min_height(ADDRESS_HEIGHT);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                match address_security_icon_for_location(location) {
                    AddressSecurityIcon::Slate { icon, color } => {
                        let page_info_icon = slate_icons.texture(ui.ctx(), icon, color);
                        let (slot_rect, _) = ui.allocate_exact_size(
                            egui::Vec2::splat(ADDRESS_SECURITY_ICON_SIZE),
                            egui::Sense::hover(),
                        );
                        let icon_rect = address_slate_security_icon_rect(slot_rect);
                        if ui.is_rect_visible(slot_rect) {
                            ui.painter().image(
                                page_info_icon.id,
                                icon_rect,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                    AddressSecurityIcon::Raster(raster) => {
                        let page_info_icon = slate_icons.raster_mask_texture(
                            ui.ctx(),
                            raster,
                            address_security_raster_color(raster),
                        );
                        ui.add(Gui::icon_image(page_info_icon, ADDRESS_SECURITY_ICON_SIZE));
                    }
                }
                ui.add_space(ADDRESS_ICON_GAP);

                let text_width = (ui.available_width() - ADDRESS_BOOKMARK_RESERVED_WIDTH).max(80.0);
                let _ = ui.add_sized(
                    [text_width, ADDRESS_TEXT_HEIGHT],
                    egui::TextEdit::singleline(location)
                        .id(egui::Id::new("headless_location_input"))
                        .font(egui::FontId::proportional(ADDRESS_INPUT_TEXT_SIZE))
                        .frame(egui::Frame::NONE)
                        .hint_text("Search the web or enter an address"),
                );

                let bookmark_icon = slate_icons.raster_mask_texture(
                    ui.ctx(),
                    SlateRaster::BookmarkAdd,
                    address_bookmark_icon_color(),
                );
                let _ =
                    Gui::address_raster_button_sized(ui, bookmark_icon, ADDRESS_BOOKMARK_ICON_SIZE);
            });
        });
}

fn render_footer(root_ui: &mut egui::Ui, load_status: LoadStatus) -> egui::Rect {
    let footer_frame = egui::Frame::NONE
        .fill(chrome_panel_background_color())
        .inner_margin(footer_panel_margin());
    egui::Panel::bottom("headless_footer")
        .exact_size(FOOTER_HEIGHT)
        .frame(footer_frame)
        .show_separator_line(false)
        .show_inside(root_ui, |ui| {
            Gui::draw_footer(
                ui,
                load_status,
                &BroadwebStatusSnapshot::idle(),
                "slate://home",
            )
        })
        .response
        .rect
}

fn render_home_panel(
    root_ui: &mut egui::Ui,
    slate_icons: &mut SlateIconCache,
    home_search: &mut String,
) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(home_view_background_color()))
        .show_inside(root_ui, |ui| {
            let home_rect = ui.max_rect();
            ui.set_min_size(home_rect.size());
            ui.painter()
                .rect_filled(home_rect, 0.0, home_view_background_color());
            let response = ui
                .scope_builder(egui::UiBuilder::new().max_rect(home_rect), |ui| {
                    Gui::draw_home_content(
                        ui,
                        home_rect,
                        slate_icons,
                        &std::collections::HashMap::new(),
                        home_search,
                        &default_home_bookmark_cards(),
                    )
                })
                .inner;
            let _ = response.layout;
            let _ = response.navigation_request;
        });
}

#[derive(Clone, Copy, Debug)]
struct SnapshotChromeGeometry {
    rail_button_rects: [egui::Rect; 5],
    rail_web_tab_row_rects: [egui::Rect; 3],
    rail_web_close_button_rects: [egui::Rect; 3],
    rail_web_new_tab_row_rect: egui::Rect,
    rail_download_progress_rects: [egui::Rect; 2],
    toolbar_rect: egui::Rect,
    toolbar_content_rect: egui::Rect,
    footer_rect: egui::Rect,
}

#[derive(Clone, Copy, Debug)]
struct SnapshotToolbarControlsGeometry {
    nav_button_rects: [egui::Rect; 3],
    nav_icon_rects: [egui::Rect; 3],
    nav_stop_icon_rect: egui::Rect,
    address_rect: egui::Rect,
    address_security_icon_rect: egui::Rect,
    address_bookmark_icon_rect: egui::Rect,
    privacy_button_rect: egui::Rect,
    separator_rect: egui::Rect,
    menu_button_rect: egui::Rect,
}

#[derive(Clone, Copy, Debug)]
enum VerificationSource {
    Complete,
    Loading,
    HoverNavBack,
    HoverNavReload,
    HoverMenu,
    ExpandedRail,
}

impl VerificationSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Loading => "loading",
            Self::HoverNavBack => "hover-nav-back",
            Self::HoverNavReload => "hover-nav-reload",
            Self::HoverMenu => "hover-menu",
            Self::ExpandedRail => "expanded-rail",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VerificationRegion {
    name: &'static str,
    file_name: &'static str,
    source: VerificationSource,
    rect: egui::Rect,
    purpose: &'static str,
    monitor: RegionMonitor,
}

#[derive(Clone, Copy, Debug)]
struct RegionMonitor {
    min_detail_pixels: u64,
    min_dark_pixels: u64,
    min_detail_width: u32,
    min_detail_height: u32,
    min_vertical_detail_columns: Option<u32>,
    warn_vertical_detail_columns_above: Option<u32>,
    manual_review: &'static [&'static str],
}

#[derive(Clone, Debug)]
struct RegionMonitorEvaluation {
    status: MonitorStatus,
    failures: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MonitorStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Copy, Debug, Default)]
struct VerificationSummary {
    regions: u32,
    passed: u32,
    warned: u32,
    failed: u32,
    manual_review_regions: u32,
}

#[derive(Clone, Copy, Debug)]
struct RegionCapture {
    rect: PixelRect,
    metrics: CropMetrics,
}

#[derive(Clone, Copy, Debug, Default)]
struct CropMetrics {
    total_pixels: u64,
    opaque_pixels: u64,
    transparent_pixels: u64,
    detail_pixels: u64,
    dark_pixels: u64,
    left_edge_dark_pixels: u64,
    vertical_detail_columns: u32,
    average_rgb: [u8; 3],
    detail_bounds: Option<PixelRect>,
}

fn verification_regions() -> Vec<VerificationRegion> {
    let chrome = snapshot_chrome_geometry();
    let expanded_chrome = snapshot_chrome_geometry_with_rail_width(APP_RAIL_MAX_WIDTH);
    let toolbar = snapshot_toolbar_controls_geometry(chrome);

    vec![
        verification_region(
            "rail-home-button",
            "rail-home-button.png",
            chrome.rail_button_rects[0],
            "tabless Home rail tile fill, label, and icon",
        ),
        verification_region(
            "rail-web-button",
            "rail-web-button.png",
            chrome.rail_button_rects[1],
            "selected Web rail tile fill, label, icon, and tiny tab rows",
        ),
        verification_region(
            "rail-web-tab-previews",
            "rail-web-tab-previews.png",
            egui::Rect::from_min_max(
                egui::pos2(
                    chrome.rail_button_rects[1].left(),
                    chrome.rail_web_tab_row_rects[0].top() - 2.0,
                ),
                egui::pos2(
                    chrome.rail_button_rects[1].right(),
                    chrome.rail_web_new_tab_row_rect.bottom() + 2.0,
                ),
            ),
            "selected Web app's favicon and tiny title tab rows",
        ),
        verification_region(
            "rail-web-tab-close-buttons",
            "rail-web-tab-close-buttons.png",
            egui::Rect::from_min_max(
                egui::pos2(
                    chrome.rail_web_close_button_rects[0].left() - 2.0,
                    chrome.rail_web_close_button_rects[0].top() - 2.0,
                ),
                egui::pos2(
                    chrome.rail_web_close_button_rects[2].right() + 2.0,
                    chrome.rail_web_close_button_rects[2].bottom() + 2.0,
                ),
            ),
            "right-edge close buttons in selected Web rail tab rows",
        ),
        verification_region_from_source(
            VerificationSource::ExpandedRail,
            "expanded-rail-web-button",
            "expanded-rail-web-button.png",
            expanded_chrome.rail_button_rects[1],
            "selected Web rail tile at maximum resized width",
        ),
        verification_region_from_source(
            VerificationSource::ExpandedRail,
            "expanded-rail-web-tab-previews",
            "expanded-rail-web-tab-previews.png",
            egui::Rect::from_min_max(
                egui::pos2(
                    expanded_chrome.rail_button_rects[1].left(),
                    expanded_chrome.rail_web_tab_row_rects[0].top() - 2.0,
                ),
                egui::pos2(
                    expanded_chrome.rail_button_rects[1].right(),
                    expanded_chrome.rail_web_new_tab_row_rect.bottom() + 2.0,
                ),
            ),
            "expanded Web rail tab rows with scaled height, icons, and font",
        ),
        verification_region(
            "rail-downloads-button",
            "rail-downloads-button.png",
            chrome.rail_button_rects[2],
            "unselected Downloads rail tile with collapsed progress-only tab lines",
        ),
        verification_region(
            "rail-download-progress-lines",
            "rail-download-progress-lines.png",
            egui::Rect::from_min_max(
                egui::pos2(
                    chrome.rail_button_rects[2].left(),
                    chrome.rail_download_progress_rects[0].top() - 2.0,
                ),
                egui::pos2(
                    chrome.rail_button_rects[2].right(),
                    chrome.rail_download_progress_rects[1].bottom() + 2.0,
                ),
            ),
            "collapsed Downloads tab progress slivers without titles",
        ),
        verification_region(
            "rail-home-icon",
            "rail-home-icon.png",
            snapshot_rail_icon_rect(chrome.rail_button_rects[0]).expand(4.0),
            "Home rail raster icon crop",
        ),
        verification_region(
            "rail-web-icon",
            "rail-web-icon.png",
            snapshot_rail_icon_rect(chrome.rail_button_rects[1]).expand(4.0),
            "Web rail vector icon crop",
        ),
        verification_region(
            "rail-downloads-icon",
            "rail-downloads-icon.png",
            snapshot_rail_icon_rect(chrome.rail_button_rects[2]).expand(4.0),
            "Downloads rail vector icon crop",
        ),
        verification_region(
            "rail-calendar-icon",
            "rail-calendar-icon.png",
            snapshot_rail_icon_rect(chrome.rail_button_rects[3]).expand(4.0),
            "Calendar rail vector icon crop",
        ),
        verification_region(
            "rail-chat-icon",
            "rail-chat-icon.png",
            snapshot_rail_icon_rect(chrome.rail_button_rects[4]).expand(4.0),
            "Chat rail vector icon crop",
        ),
        verification_region(
            "toolbar",
            "toolbar.png",
            chrome.toolbar_rect,
            "navigation toolbar band and control spacing",
        ),
        verification_region(
            "nav-back-icon",
            "nav-back-icon.png",
            toolbar.nav_icon_rects[0].expand(4.0),
            "back navigation vector primitive crop",
        ),
        verification_region_from_source(
            VerificationSource::HoverNavBack,
            "nav-back-hover-button",
            "nav-back-hover-button.png",
            toolbar.nav_button_rects[0].expand(4.0),
            "back navigation hover shade and vector primitive alignment",
        ),
        verification_region(
            "nav-forward-icon",
            "nav-forward-icon.png",
            toolbar.nav_icon_rects[1].expand(4.0),
            "forward navigation vector primitive crop",
        ),
        verification_region(
            "nav-reload-icon",
            "nav-reload-icon.png",
            toolbar.nav_icon_rects[2].expand(4.0),
            "reload navigation vector primitive crop",
        ),
        verification_region_from_source(
            VerificationSource::HoverNavReload,
            "nav-reload-hover-button",
            "nav-reload-hover-button.png",
            toolbar.nav_button_rects[2].expand(4.0),
            "reload navigation hover shade and vector primitive alignment",
        ),
        verification_region_from_source(
            VerificationSource::Loading,
            "nav-stop-icon",
            "nav-stop-icon.png",
            toolbar.nav_stop_icon_rect.expand(4.0),
            "loading-state Stop navigation vector primitive crop",
        ),
        verification_region(
            "address-field",
            "address-field.png",
            toolbar.address_rect,
            "address field border, security icon, text, and bookmark affordance",
        ),
        verification_region(
            "address-security-icon",
            "address-security-icon.png",
            toolbar.address_security_icon_rect.expand(4.0),
            "address-field shield vector crop",
        ),
        verification_region(
            "address-bookmark-icon",
            "address-bookmark-icon.png",
            toolbar.address_bookmark_icon_rect.expand(4.0),
            "address bookmark raster crop",
        ),
        verification_region(
            "privacy-shield",
            "privacy-shield.png",
            toolbar.privacy_button_rect.expand(4.0),
            "toolbar privacy shield vector crop",
        ),
        verification_region(
            "toolbar-separator",
            "toolbar-separator.png",
            toolbar.separator_rect.expand(4.0),
            "toolbar separator line crop",
        ),
        verification_region(
            "toolbar-menu",
            "toolbar-menu.png",
            toolbar.menu_button_rect.expand(4.0),
            "three-line toolbar menu crop",
        ),
        verification_region_from_source(
            VerificationSource::HoverMenu,
            "toolbar-menu-hover-button",
            "toolbar-menu-hover-button.png",
            toolbar.menu_button_rect.expand(4.0),
            "toolbar menu hover shade and hamburger glyph alignment",
        ),
        verification_region(
            "footer-status",
            "footer-status.png",
            chrome.footer_rect,
            "footer status text and broadweb status indicator",
        ),
    ]
}

fn verification_region(
    name: &'static str,
    file_name: &'static str,
    rect: egui::Rect,
    purpose: &'static str,
) -> VerificationRegion {
    verification_region_from_source(VerificationSource::Complete, name, file_name, rect, purpose)
}

fn verification_region_from_source(
    source: VerificationSource,
    name: &'static str,
    file_name: &'static str,
    rect: egui::Rect,
    purpose: &'static str,
) -> VerificationRegion {
    VerificationRegion {
        name,
        file_name,
        source,
        rect,
        purpose,
        monitor: monitor_for_region(name),
    }
}

impl RegionMonitor {
    fn new(min_detail_pixels: u64, min_detail_width: u32, min_detail_height: u32) -> Self {
        Self {
            min_detail_pixels,
            min_dark_pixels: 0,
            min_detail_width,
            min_detail_height,
            min_vertical_detail_columns: None,
            warn_vertical_detail_columns_above: None,
            manual_review: &[],
        }
    }

    #[cfg(test)]
    fn with_dark_pixels(mut self, min_dark_pixels: u64) -> Self {
        self.min_dark_pixels = min_dark_pixels;
        self
    }

    fn with_vertical_detail(
        mut self,
        min_vertical_detail_columns: Option<u32>,
        warn_vertical_detail_columns_above: Option<u32>,
    ) -> Self {
        self.min_vertical_detail_columns = min_vertical_detail_columns;
        self.warn_vertical_detail_columns_above = warn_vertical_detail_columns_above;
        self
    }

    fn with_manual_review(mut self, manual_review: &'static [&'static str]) -> Self {
        self.manual_review = manual_review;
        self
    }
}

impl MonitorStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

impl VerificationSummary {
    fn record(&mut self, monitor: &RegionMonitorEvaluation, expectations: RegionMonitor) {
        self.regions += 1;
        match monitor.status {
            MonitorStatus::Pass => self.passed += 1,
            MonitorStatus::Warn => self.warned += 1,
            MonitorStatus::Fail => self.failed += 1,
        }
        if !expectations.manual_review.is_empty() {
            self.manual_review_regions += 1;
        }
    }
}

fn monitor_for_region(name: &'static str) -> RegionMonitor {
    match name {
        "rail-home-button" => RegionMonitor::new(40, 8, 8)
            .with_manual_review(&["confirm Home remains a tabless singleton rail target"]),
        "rail-web-button" => RegionMonitor::new(40, 8, 8)
            .with_manual_review(&["confirm selected Web tile shows tiny favicon/title tab rows"]),
        "rail-web-tab-previews" => RegionMonitor::new(16, 8, 8).with_manual_review(&[
            "confirm selected Web tab titles stay readable without crowding the rail",
        ]),
        "rail-web-tab-close-buttons" => RegionMonitor::new(8, 4, 12).with_manual_review(&[
            "confirm mini tab close buttons are visible but do not overpower the tiny labels",
        ]),
        "expanded-rail-web-button" => RegionMonitor::new(80, 24, 24).with_manual_review(&[
            "confirm maximum rail width still reads as one selected Web app group",
        ]),
        "expanded-rail-web-tab-previews" => RegionMonitor::new(32, 24, 12).with_manual_review(&[
            "confirm resized rail increases mini-tab row height, icon size, and font without clipping",
        ]),
        "rail-downloads-button" => RegionMonitor::new(32, 8, 8).with_manual_review(&[
            "confirm unselected Downloads keeps progress-only collapsed tab lines",
        ]),
        "rail-download-progress-lines" => RegionMonitor::new(4, 20, 1).with_manual_review(&[
            "confirm collapsed Downloads rows show progress without file titles",
        ]),
        "toolbar" => RegionMonitor::new(64, 20, 8),
        "nav-back-hover-button" | "nav-reload-hover-button" => RegionMonitor::new(64, 18, 18)
            .with_manual_review(&["confirm hover shade is centered behind the navigation glyph"]),
        "address-field" => RegionMonitor::new(64, 20, 8),
        "footer-status" => RegionMonitor::new(24, 8, 5),
        "toolbar-separator" => RegionMonitor::new(8, 1, 12).with_vertical_detail(Some(1), Some(3)),
        "rail-home-icon" => RegionMonitor::new(8, 4, 4)
            .with_manual_review(&["compare Home raster weight against the navigation theme"]),
        "rail-web-icon"
        | "rail-downloads-icon"
        | "rail-calendar-icon"
        | "rail-chat-icon"
        | "nav-back-icon"
        | "nav-forward-icon"
        | "nav-reload-icon"
        | "nav-stop-icon"
        | "address-security-icon"
        | "address-bookmark-icon"
        | "privacy-shield"
        | "toolbar-menu" => RegionMonitor::new(8, 4, 4),
        _ => RegionMonitor::new(1, 1, 1),
    }
}

fn evaluate_region_monitor(
    expectations: RegionMonitor,
    crop_rect: PixelRect,
    metrics: CropMetrics,
) -> RegionMonitorEvaluation {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    if crop_rect.width() == 0 || crop_rect.height() == 0 {
        failures.push("crop rectangle is empty".to_owned());
    }
    if metrics.total_pixels == 0 {
        failures.push("crop has no pixels".to_owned());
    }
    if metrics.opaque_pixels == 0 {
        failures.push("crop has no opaque pixels".to_owned());
    }
    if metrics.detail_pixels < expectations.min_detail_pixels {
        failures.push(format!(
            "detail_pixels {} is below minimum {}",
            metrics.detail_pixels, expectations.min_detail_pixels
        ));
    }
    if metrics.dark_pixels < expectations.min_dark_pixels {
        failures.push(format!(
            "dark_pixels {} is below minimum {}",
            metrics.dark_pixels, expectations.min_dark_pixels
        ));
    }

    match metrics.detail_bounds {
        Some(bounds) => {
            if bounds.width() < expectations.min_detail_width {
                failures.push(format!(
                    "detail width {} is below minimum {}",
                    bounds.width(),
                    expectations.min_detail_width
                ));
            }
            if bounds.height() < expectations.min_detail_height {
                failures.push(format!(
                    "detail height {} is below minimum {}",
                    bounds.height(),
                    expectations.min_detail_height
                ));
            }
        }
        None => failures.push("crop has no detail bounds".to_owned()),
    }

    if let Some(minimum) = expectations.min_vertical_detail_columns {
        if metrics.vertical_detail_columns < minimum {
            failures.push(format!(
                "vertical_detail_columns {} is below minimum {minimum}",
                metrics.vertical_detail_columns
            ));
        }
    }
    if let Some(maximum) = expectations.warn_vertical_detail_columns_above {
        if metrics.vertical_detail_columns > maximum {
            warnings.push(format!(
                "vertical_detail_columns {} is above review threshold {maximum}",
                metrics.vertical_detail_columns
            ));
        }
    }

    let status = if !failures.is_empty() {
        MonitorStatus::Fail
    } else if !warnings.is_empty() {
        MonitorStatus::Warn
    } else {
        MonitorStatus::Pass
    };

    RegionMonitorEvaluation {
        status,
        failures,
        warnings,
    }
}

fn snapshot_chrome_geometry() -> SnapshotChromeGeometry {
    snapshot_chrome_geometry_with_rail_width(APP_RAIL_WIDTH)
}

fn snapshot_chrome_geometry_with_rail_width(rail_width: f32) -> SnapshotChromeGeometry {
    let viewport_width = DEFAULT_SNAPSHOT_WIDTH as f32;
    let viewport_height = DEFAULT_SNAPSHOT_HEIGHT as f32;
    let central_width = viewport_width - rail_width;
    let toolbar_rect = egui::Rect::from_min_size(
        egui::pos2(rail_width, 0.0),
        egui::vec2(central_width, TOOLBAR_HEIGHT),
    );
    let rail_button_left = f32::from(RAIL_PANEL_MARGIN_X);
    let first_rail_button_top = RAIL_TOP_SPACE;
    let rail_button_w = rail_button_width(rail_width - f32::from(RAIL_PANEL_MARGIN_X) * 2.0);
    let home_button_rect = egui::Rect::from_min_size(
        egui::pos2(rail_button_left, first_rail_button_top),
        egui::vec2(rail_button_w, RAIL_BUTTON_SIZE),
    );
    let web_button_rect = egui::Rect::from_min_size(
        egui::pos2(rail_button_left, home_button_rect.bottom() + RAIL_ITEM_GAP),
        egui::vec2(rail_button_w, rail_web_item_height(true, 3, rail_button_w)),
    );
    let downloads_button_rect = egui::Rect::from_min_size(
        egui::pos2(rail_button_left, web_button_rect.bottom() + RAIL_ITEM_GAP),
        egui::vec2(rail_button_w, rail_item_height(false, 2, rail_button_w)),
    );
    let calendar_button_rect = egui::Rect::from_min_size(
        egui::pos2(
            rail_button_left,
            downloads_button_rect.bottom() + RAIL_ITEM_GAP,
        ),
        egui::vec2(rail_button_w, RAIL_BUTTON_SIZE),
    );
    let chat_button_rect = egui::Rect::from_min_size(
        egui::pos2(
            rail_button_left,
            calendar_button_rect.bottom() + RAIL_ITEM_GAP,
        ),
        egui::vec2(rail_button_w, RAIL_BUTTON_SIZE),
    );
    let rail_button_rects = [
        home_button_rect,
        web_button_rect,
        downloads_button_rect,
        calendar_button_rect,
        chat_button_rect,
    ];
    let rail_web_tab_row_rects = [
        rail_tab_row_rect(web_button_rect, 0),
        rail_tab_row_rect(web_button_rect, 1),
        rail_tab_row_rect(web_button_rect, 2),
    ];
    let rail_web_close_button_rects = [
        rail_tab_close_button_rect(rail_web_tab_row_rects[0]),
        rail_tab_close_button_rect(rail_web_tab_row_rects[1]),
        rail_tab_close_button_rect(rail_web_tab_row_rects[2]),
    ];
    let rail_web_new_tab_row_rect = rail_tab_row_rect(web_button_rect, 3);
    let rail_download_progress_rects = [
        rail_collapsed_tab_line_rect(downloads_button_rect, 0),
        rail_collapsed_tab_line_rect(downloads_button_rect, 1),
    ];

    SnapshotChromeGeometry {
        rail_button_rects,
        rail_web_tab_row_rects,
        rail_web_close_button_rects,
        rail_web_new_tab_row_rect,
        rail_download_progress_rects,
        toolbar_rect,
        toolbar_content_rect: toolbar_rect.shrink2(egui::vec2(
            f32::from(TOOLBAR_PANEL_MARGIN_X),
            f32::from(TOOLBAR_PANEL_MARGIN_Y),
        )),
        footer_rect: egui::Rect::from_min_size(
            egui::pos2(rail_width, viewport_height - FOOTER_HEIGHT),
            egui::vec2(central_width, FOOTER_HEIGHT),
        ),
    }
}

fn snapshot_toolbar_controls_geometry(
    chrome: SnapshotChromeGeometry,
) -> SnapshotToolbarControlsGeometry {
    let toolbar_content_rect = chrome.toolbar_content_rect;
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
    let nav_stop_icon_rect = egui::Rect::from_center_size(
        nav_button_rects[2].center(),
        egui::Vec2::splat(TOOLBAR_ICON_SIZE),
    );
    let address_left = nav_button_rects[2].right() + TOOLBAR_ITEM_SPACING + ADDRESS_LEADING_GAP;
    let address_available_width = (toolbar_content_rect.right() - address_left).max(0.0);
    let address_content_width = toolbar_address_width(address_available_width);
    let address_rect = egui::Rect::from_min_size(
        egui::pos2(address_left, center_y - ADDRESS_HEIGHT / 2.0),
        egui::vec2(
            address_content_width + f32::from(ADDRESS_INNER_MARGIN_X) * 2.0,
            ADDRESS_HEIGHT,
        ),
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

    SnapshotToolbarControlsGeometry {
        nav_button_rects,
        nav_icon_rects,
        nav_stop_icon_rect,
        address_rect,
        address_security_icon_rect: address_slate_security_icon_rect(address_security_slot_rect),
        address_bookmark_icon_rect: egui::Rect::from_center_size(
            address_bookmark_button_rect.center(),
            egui::Vec2::splat(ADDRESS_BOOKMARK_ICON_SIZE),
        ),
        privacy_button_rect,
        separator_rect,
        menu_button_rect,
    }
}

fn snapshot_rail_icon_rect(button_rect: egui::Rect) -> egui::Rect {
    rail_icon_slot_rect(button_rect)
}

fn crop_metrics(crop: &RgbaImage) -> CropMetrics {
    if crop.width() == 0 || crop.height() == 0 {
        return CropMetrics::default();
    }

    let background = crop.get_pixel(0, 0).0;
    let mut metrics = CropMetrics {
        total_pixels: u64::from(crop.width()) * u64::from(crop.height()),
        ..Default::default()
    };
    let mut red_sum = 0_u64;
    let mut green_sum = 0_u64;
    let mut blue_sum = 0_u64;
    let mut detail_min_x = crop.width();
    let mut detail_min_y = crop.height();
    let mut detail_max_x = 0;
    let mut detail_max_y = 0;

    for (x, y, pixel) in crop.enumerate_pixels() {
        let [red, green, blue, alpha] = pixel.0;
        if alpha == 0 {
            metrics.transparent_pixels += 1;
            continue;
        }

        metrics.opaque_pixels += 1;
        red_sum += u64::from(red);
        green_sum += u64::from(green);
        blue_sum += u64::from(blue);

        if alpha > 180 && u16::from(red) + u16::from(green) + u16::from(blue) < 330 {
            metrics.dark_pixels += 1;
            if x < 8 {
                metrics.left_edge_dark_pixels += 1;
            }
        }

        if alpha > 32 && color_distance(pixel.0, background) > 24 {
            metrics.detail_pixels += 1;
            detail_min_x = detail_min_x.min(x);
            detail_min_y = detail_min_y.min(y);
            detail_max_x = detail_max_x.max(x + 1);
            detail_max_y = detail_max_y.max(y + 1);
        }
    }

    if metrics.opaque_pixels > 0 {
        metrics.average_rgb = [
            (red_sum / metrics.opaque_pixels) as u8,
            (green_sum / metrics.opaque_pixels) as u8,
            (blue_sum / metrics.opaque_pixels) as u8,
        ];
    }

    if metrics.detail_pixels > 0 {
        metrics.detail_bounds = Some(PixelRect {
            min_x: detail_min_x,
            min_y: detail_min_y,
            max_x: detail_max_x,
            max_y: detail_max_y,
        });
    }

    metrics.vertical_detail_columns = vertical_detail_columns(crop, background);
    metrics
}

fn vertical_detail_columns(crop: &RgbaImage, background: [u8; 4]) -> u32 {
    if crop.height() == 0 {
        return 0;
    }

    let threshold = (crop.height() * 3 / 4).max(1);
    let mut columns = 0;
    for x in 0..crop.width() {
        let detail_pixels = (0..crop.height())
            .filter(|y| {
                let pixel = crop.get_pixel(x, *y).0;
                pixel[3] > 32 && color_distance(pixel, background) > 24
            })
            .count() as u32;
        if detail_pixels >= threshold {
            columns += 1;
        }
    }
    columns
}

fn color_distance(left: [u8; 4], right: [u8; 4]) -> u16 {
    u16::from(left[0].abs_diff(right[0]))
        + u16::from(left[1].abs_diff(right[1]))
        + u16::from(left[2].abs_diff(right[2]))
        + u16::from(left[3].abs_diff(right[3]))
}

fn pixel_rect_json(rect: PixelRect) -> serde_json::Value {
    serde_json::json!({
        "x": rect.min_x,
        "y": rect.min_y,
        "width": rect.width(),
        "height": rect.height(),
    })
}

fn crop_metrics_json(metrics: CropMetrics) -> serde_json::Value {
    serde_json::json!({
        "total_pixels": metrics.total_pixels,
        "opaque_pixels": metrics.opaque_pixels,
        "transparent_pixels": metrics.transparent_pixels,
        "detail_pixels": metrics.detail_pixels,
        "dark_pixels": metrics.dark_pixels,
        "left_edge_dark_pixels": metrics.left_edge_dark_pixels,
        "vertical_detail_columns": metrics.vertical_detail_columns,
        "average_rgb": metrics.average_rgb,
        "detail_bounds": metrics.detail_bounds.map(pixel_rect_json),
    })
}

fn automated_review_json(regions: &HashMap<&'static str, RegionCapture>) -> serde_json::Value {
    let mut findings = Vec::new();

    if let Some(selected_rail) = regions.get("rail-web-button") {
        if selected_rail.metrics.left_edge_dark_pixels < 24 {
            findings.push(review_finding_json(
                "warning",
                "rail-web-button",
                "selected rail state has no strong edge affordance",
                serde_json::json!({
                    "left_edge_dark_pixels": selected_rail.metrics.left_edge_dark_pixels,
                    "minimum": 24,
                }),
                "Add a compact accent mark or stronger selected fill so the active app is visible during scanning.",
            ));
        }
    }

    if let Some(footer) = regions.get("footer-status") {
        let density = detail_density(footer.metrics);
        if density < 0.012 {
            findings.push(review_finding_json(
                "info",
                "footer-status",
                "idle footer has sparse content for its footprint",
                serde_json::json!({
                    "detail_density": rounded_ratio(density),
                    "pixels": {
                        "width": footer.rect.width(),
                        "height": footer.rect.height(),
                    },
                }),
                "Consider collapsing the idle footer or reserving the band for loading, broadweb routing, hover previews, and warnings.",
            ));
        }
    }

    if let Some((nav_density, right_density)) = toolbar_control_density(regions) {
        if right_density < nav_density * 0.7 {
            findings.push(review_finding_json(
                "info",
                "toolbar",
                "right toolbar controls are visually quieter than navigation controls",
                serde_json::json!({
                    "navigation_detail_density": rounded_ratio(nav_density),
                    "right_control_detail_density": rounded_ratio(right_density),
                }),
                "Increase contrast or tighten spacing for bookmark, privacy, and menu controls after the primary navigation cluster is stable.",
            ));
        }
    }

    findings.extend(nav_hover_alignment_findings(regions));

    let warning_count = findings
        .iter()
        .filter(|finding| {
            finding
                .get("severity")
                .and_then(|severity| severity.as_str())
                == Some("warning")
        })
        .count();
    let info_count = findings
        .iter()
        .filter(|finding| {
            finding
                .get("severity")
                .and_then(|severity| severity.as_str())
                == Some("info")
        })
        .count();

    serde_json::json!({
        "schema": "slate.chrome.automated-review.v1",
        "summary": {
            "findings": findings.len(),
            "warnings": warning_count,
            "info": info_count,
        },
        "rules": [
            "selected rail buttons should expose a measurable edge affordance",
            "large idle chrome bands should contain enough visible detail to justify their footprint",
            "secondary toolbar controls should not become much quieter than primary navigation controls",
            "navigation hover shades should stay centered under their glyphs"
        ],
        "findings": findings,
    })
}

fn nav_hover_alignment_findings(
    regions: &HashMap<&'static str, RegionCapture>,
) -> Vec<serde_json::Value> {
    [
        ("nav-back-icon", "nav-back-hover-button", "Back"),
        ("nav-reload-icon", "nav-reload-hover-button", "Reload"),
    ]
    .into_iter()
    .filter_map(|(icon_region, button_region, label)| {
        let icon = regions.get(icon_region)?;
        let button = regions.get(button_region)?;
        let delta_x = pixel_rect_center_x(icon.rect) - pixel_rect_center_x(button.rect);
        if delta_x.abs() <= 1.5 {
            return None;
        }

        Some(review_finding_json(
            "warning",
            button_region,
            "navigation hover shade is offset from glyph",
            serde_json::json!({
                "control": label,
                "center_delta_x": rounded_ratio(delta_x),
                "maximum_abs_delta_x": 1.5,
            }),
            "Center the navigation glyph in the hover button rect so hover feedback and the click target read as one control.",
        ))
    })
    .collect()
}

fn review_finding_json(
    severity: &'static str,
    region: &'static str,
    title: &'static str,
    evidence: serde_json::Value,
    recommendation: &'static str,
) -> serde_json::Value {
    serde_json::json!({
        "severity": severity,
        "region": region,
        "title": title,
        "evidence": evidence,
        "recommendation": recommendation,
    })
}

fn detail_density(metrics: CropMetrics) -> f32 {
    if metrics.total_pixels == 0 {
        return 0.0;
    }
    metrics.detail_pixels as f32 / metrics.total_pixels as f32
}

fn rounded_ratio(value: f32) -> f64 {
    (f64::from(value) * 10_000.0).round() / 10_000.0
}

fn pixel_rect_center_x(rect: PixelRect) -> f32 {
    (rect.min_x as f32 + rect.max_x as f32) / 2.0
}

fn toolbar_control_density(regions: &HashMap<&'static str, RegionCapture>) -> Option<(f32, f32)> {
    let nav_density = average_region_density(
        regions,
        &["nav-back-icon", "nav-forward-icon", "nav-reload-icon"],
    )?;
    let right_density = average_region_density(
        regions,
        &["address-bookmark-icon", "privacy-shield", "toolbar-menu"],
    )?;
    Some((nav_density, right_density))
}

fn average_region_density(
    regions: &HashMap<&'static str, RegionCapture>,
    names: &[&'static str],
) -> Option<f32> {
    let mut total = 0.0;
    for name in names {
        total += detail_density(regions.get(name)?.metrics);
    }
    Some(total / names.len() as f32)
}

fn region_monitor_json(
    expectations: RegionMonitor,
    evaluation: &RegionMonitorEvaluation,
) -> serde_json::Value {
    serde_json::json!({
        "status": evaluation.status.as_str(),
        "expectations": {
            "min_detail_pixels": expectations.min_detail_pixels,
            "min_dark_pixels": expectations.min_dark_pixels,
            "min_detail_width": expectations.min_detail_width,
            "min_detail_height": expectations.min_detail_height,
            "min_vertical_detail_columns": expectations.min_vertical_detail_columns,
            "warn_vertical_detail_columns_above": expectations.warn_vertical_detail_columns_above,
        },
        "failures": &evaluation.failures,
        "warnings": &evaluation.warnings,
        "manual_review": expectations.manual_review,
    })
}

fn verification_summary_json(summary: VerificationSummary) -> serde_json::Value {
    serde_json::json!({
        "regions": summary.regions,
        "passed": summary.passed,
        "warned": summary.warned,
        "failed": summary.failed,
        "manual_review_regions": summary.manual_review_regions,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct PremulRgba {
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
}

impl PremulRgba {
    const WHITE: Self = Self {
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    };

    fn from_color32(color: Color32) -> Self {
        let [red, green, blue, alpha] = color.to_array();
        Self {
            red: f32::from(red) / 255.0,
            green: f32::from(green) / 255.0,
            blue: f32::from(blue) / 255.0,
            alpha: f32::from(alpha) / 255.0,
        }
    }

    fn multiply(self, other: Self) -> Self {
        Self {
            red: self.red * other.red,
            green: self.green * other.green,
            blue: self.blue * other.blue,
            alpha: self.alpha * other.alpha,
        }
    }

    fn lerp(self, other: Self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        let inverse = 1.0 - factor;
        Self {
            red: self.red * inverse + other.red * factor,
            green: self.green * inverse + other.green * factor,
            blue: self.blue * inverse + other.blue * factor,
            alpha: self.alpha * inverse + other.alpha * factor,
        }
    }

    fn blend_over(self, destination: Self) -> Self {
        let inverse_alpha = 1.0 - self.alpha;
        Self {
            red: self.red + destination.red * inverse_alpha,
            green: self.green + destination.green * inverse_alpha,
            blue: self.blue + destination.blue * inverse_alpha,
            alpha: self.alpha + destination.alpha * inverse_alpha,
        }
    }

    fn to_unmultiplied_rgba(self) -> [u8; 4] {
        let alpha = self.alpha.clamp(0.0, 1.0);
        if alpha <= f32::EPSILON {
            return [0, 0, 0, 0];
        }

        [
            float_to_u8(self.red / alpha),
            float_to_u8(self.green / alpha),
            float_to_u8(self.blue / alpha),
            float_to_u8(alpha),
        ]
    }
}

fn float_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Clone, Debug)]
struct HeadlessTexture {
    width: usize,
    height: usize,
    pixels: Vec<PremulRgba>,
}

impl HeadlessTexture {
    fn from_color_image(image: &egui::ColorImage) -> Self {
        Self {
            width: image.size[0],
            height: image.size[1],
            pixels: image
                .pixels
                .iter()
                .copied()
                .map(PremulRgba::from_color32)
                .collect(),
        }
    }

    fn patch(&mut self, position: [usize; 2], image: &egui::ColorImage) -> Result<(), String> {
        let [left, top] = position;
        let width = image.size[0];
        let height = image.size[1];
        if left + width > self.width || top + height > self.height {
            return Err(format!(
                "texture patch {}x{} at {},{} exceeds {}x{}",
                width, height, left, top, self.width, self.height
            ));
        }

        for y in 0..height {
            let destination_start = (top + y) * self.width + left;
            let source_start = y * width;
            for x in 0..width {
                self.pixels[destination_start + x] =
                    PremulRgba::from_color32(image.pixels[source_start + x]);
            }
        }
        Ok(())
    }

    fn sample(&self, uv: egui::Pos2) -> PremulRgba {
        if self.width == 0 || self.height == 0 {
            return PremulRgba::default();
        }

        let x = (uv.x * self.width as f32 - 0.5).clamp(0.0, (self.width - 1) as f32);
        let y = (uv.y * self.height as f32 - 0.5).clamp(0.0, (self.height - 1) as f32);
        let left = x.floor() as usize;
        let top = y.floor() as usize;
        let right = (left + 1).min(self.width - 1);
        let bottom = (top + 1).min(self.height - 1);
        let horizontal = x - left as f32;
        let vertical = y - top as f32;

        let top_color = self
            .pixel(left, top)
            .lerp(self.pixel(right, top), horizontal);
        let bottom_color = self
            .pixel(left, bottom)
            .lerp(self.pixel(right, bottom), horizontal);
        top_color.lerp(bottom_color, vertical)
    }

    fn pixel(&self, x: usize, y: usize) -> PremulRgba {
        self.pixels[y * self.width + x]
    }
}

#[derive(Clone, Copy, Debug)]
struct PixelRect {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl PixelRect {
    fn from_point_rect(rect: egui::Rect, pixels_per_point: f32, width: u32, height: u32) -> Self {
        Self {
            min_x: point_to_min_pixel(rect.min.x, pixels_per_point, width),
            min_y: point_to_min_pixel(rect.min.y, pixels_per_point, height),
            max_x: point_to_max_pixel(rect.max.x, pixels_per_point, width),
            max_y: point_to_max_pixel(rect.max.y, pixels_per_point, height),
        }
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let min_x = self.min_x.max(other.min_x);
        let min_y = self.min_y.max(other.min_y);
        let max_x = self.max_x.min(other.max_x);
        let max_y = self.max_y.min(other.max_y);
        (min_x < max_x && min_y < max_y).then_some(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    fn width(self) -> u32 {
        self.max_x.saturating_sub(self.min_x)
    }

    fn height(self) -> u32 {
        self.max_y.saturating_sub(self.min_y)
    }
}

fn point_to_min_pixel(point: f32, pixels_per_point: f32, limit: u32) -> u32 {
    (point * pixels_per_point).floor().clamp(0.0, limit as f32) as u32
}

fn point_to_max_pixel(point: f32, pixels_per_point: f32, limit: u32) -> u32 {
    (point * pixels_per_point).ceil().clamp(0.0, limit as f32) as u32
}

struct SoftwareRenderer {
    width: u32,
    height: u32,
    pixels: Vec<PremulRgba>,
    textures: HashMap<TextureId, HeadlessTexture>,
}

impl SoftwareRenderer {
    fn new(size: [u32; 2]) -> Self {
        let [width, height] = size;
        Self {
            width,
            height,
            pixels: vec![PremulRgba::default(); (width as usize).saturating_mul(height as usize)],
            textures: HashMap::new(),
        }
    }

    fn apply_textures_delta(&mut self, delta: &egui::TexturesDelta) -> Result<(), String> {
        for (id, image_delta) in delta.set.iter() {
            let ImageData::Color(image) = &image_delta.image;
            if let Some(position) = image_delta.pos {
                let texture = self
                    .textures
                    .get_mut(id)
                    .ok_or_else(|| format!("partial texture update for missing {id:?}"))?;
                texture.patch(position, image)?;
            } else {
                self.textures
                    .insert(*id, HeadlessTexture::from_color_image(image));
            }
        }

        for id in delta.free.iter() {
            self.textures.remove(id);
        }

        Ok(())
    }

    fn paint(
        &mut self,
        clipped_primitives: &[ClippedPrimitive],
        pixels_per_point: f32,
    ) -> Result<(), String> {
        for clipped_primitive in clipped_primitives {
            let clip_rect = PixelRect::from_point_rect(
                clipped_primitive.clip_rect,
                pixels_per_point,
                self.width,
                self.height,
            );
            match &clipped_primitive.primitive {
                Primitive::Mesh(mesh) => {
                    for triangle in mesh.indices.chunks_exact(3) {
                        let vertex0 = mesh
                            .vertices
                            .get(triangle[0] as usize)
                            .ok_or_else(|| "egui mesh references missing vertex".to_owned())?;
                        let vertex1 = mesh
                            .vertices
                            .get(triangle[1] as usize)
                            .ok_or_else(|| "egui mesh references missing vertex".to_owned())?;
                        let vertex2 = mesh
                            .vertices
                            .get(triangle[2] as usize)
                            .ok_or_else(|| "egui mesh references missing vertex".to_owned())?;
                        self.paint_triangle(
                            mesh.texture_id,
                            [*vertex0, *vertex1, *vertex2],
                            clip_rect,
                            pixels_per_point,
                        )?;
                    }
                }
                Primitive::Callback(_) => {}
            }
        }
        Ok(())
    }

    fn paint_triangle(
        &mut self,
        texture_id: TextureId,
        vertices: [Vertex; 3],
        clip_rect: PixelRect,
        pixels_per_point: f32,
    ) -> Result<(), String> {
        let area = edge(vertices[0].pos, vertices[1].pos, vertices[2].pos);
        if area.abs() <= f32::EPSILON {
            return Ok(());
        }

        let min_x = vertices[0]
            .pos
            .x
            .min(vertices[1].pos.x)
            .min(vertices[2].pos.x);
        let min_y = vertices[0]
            .pos
            .y
            .min(vertices[1].pos.y)
            .min(vertices[2].pos.y);
        let max_x = vertices[0]
            .pos
            .x
            .max(vertices[1].pos.x)
            .max(vertices[2].pos.x);
        let max_y = vertices[0]
            .pos
            .y
            .max(vertices[1].pos.y)
            .max(vertices[2].pos.y);
        let triangle_rect = PixelRect::from_point_rect(
            egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y)),
            pixels_per_point,
            self.width,
            self.height,
        );
        let Some(bounds) = triangle_rect.intersect(clip_rect) else {
            return Ok(());
        };

        for y in bounds.min_y..bounds.max_y {
            for x in bounds.min_x..bounds.max_x {
                let sample_position = egui::pos2(
                    (x as f32 + 0.5) / pixels_per_point,
                    (y as f32 + 0.5) / pixels_per_point,
                );
                let weights = [
                    edge(vertices[1].pos, vertices[2].pos, sample_position) / area,
                    edge(vertices[2].pos, vertices[0].pos, sample_position) / area,
                    edge(vertices[0].pos, vertices[1].pos, sample_position) / area,
                ];
                if weights.iter().any(|weight| *weight < -0.0001) {
                    continue;
                }

                let uv = egui::pos2(
                    vertices[0].uv.x * weights[0]
                        + vertices[1].uv.x * weights[1]
                        + vertices[2].uv.x * weights[2],
                    vertices[0].uv.y * weights[0]
                        + vertices[1].uv.y * weights[1]
                        + vertices[2].uv.y * weights[2],
                );
                let vertex_color = interpolate_vertex_color(vertices, weights);
                let texture_color = self.sample_texture(texture_id, uv)?;
                self.blend_pixel(x, y, vertex_color.multiply(texture_color));
            }
        }

        Ok(())
    }

    fn sample_texture(&self, texture_id: TextureId, uv: egui::Pos2) -> Result<PremulRgba, String> {
        if let Some(texture) = self.textures.get(&texture_id) {
            return Ok(texture.sample(uv));
        }

        if texture_id == TextureId::default() {
            return Ok(PremulRgba::WHITE);
        }

        Err(format!("missing egui texture {texture_id:?}"))
    }

    fn blend_pixel(&mut self, x: u32, y: u32, source: PremulRgba) {
        let index = (y as usize)
            .saturating_mul(self.width as usize)
            .saturating_add(x as usize);
        if let Some(destination) = self.pixels.get_mut(index) {
            *destination = source.blend_over(*destination);
        }
    }

    fn into_image(self) -> RgbaImage {
        let mut data = Vec::with_capacity(self.pixels.len() * 4);
        for pixel in self.pixels {
            data.extend(pixel.to_unmultiplied_rgba());
        }
        RgbaImage::from_vec(self.width, self.height, data)
            .expect("headless framebuffer dimensions should match its byte length")
    }
}

fn interpolate_vertex_color(vertices: [Vertex; 3], weights: [f32; 3]) -> PremulRgba {
    let colors = [
        PremulRgba::from_color32(vertices[0].color),
        PremulRgba::from_color32(vertices[1].color),
        PremulRgba::from_color32(vertices[2].color),
    ];
    PremulRgba {
        red: colors[0].red * weights[0] + colors[1].red * weights[1] + colors[2].red * weights[2],
        green: colors[0].green * weights[0]
            + colors[1].green * weights[1]
            + colors[2].green * weights[2],
        blue: colors[0].blue * weights[0]
            + colors[1].blue * weights[1]
            + colors[2].blue * weights[2],
        alpha: colors[0].alpha * weights[0]
            + colors[1].alpha * weights[1]
            + colors[2].alpha * weights[2],
    }
}

fn edge(start: egui::Pos2, end: egui::Pos2, point: egui::Pos2) -> f32 {
    (point.x - start.x) * (end.y - start.y) - (point.y - start.y) * (end.x - start.x)
}

#[cfg(test)]
mod tests {
    use super::{
        CropMetrics, DEFAULT_SNAPSHOT_HEIGHT, DEFAULT_SNAPSHOT_WIDTH, MonitorStatus, PixelRect,
        RegionCapture, RegionMonitor, automated_review_json, crop_metrics, evaluate_region_monitor,
        nav_hover_alignment_findings, render_snapshot, verification_regions,
        write_default_verification_report,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_headless_snapshot_uses_concept_viewport() {
        assert_eq!(DEFAULT_SNAPSHOT_WIDTH, 1672);
        assert_eq!(DEFAULT_SNAPSHOT_HEIGHT, 941);
    }

    #[test]
    fn headless_snapshot_renders_chrome_pixels() {
        let image = render_snapshot([640, 360]).expect("headless chrome should render");

        assert_eq!(image.width(), 640);
        assert_eq!(image.height(), 360);
        assert!(
            image.pixels().any(|pixel| pixel.0[3] == u8::MAX),
            "snapshot should contain opaque chrome pixels"
        );
        assert!(
            image
                .pixels()
                .any(|pixel| pixel.0[0] < 245 || pixel.0[1] < 245 || pixel.0[2] < 245),
            "snapshot should contain non-background UI detail"
        );
        let viewport_pixel = image.get_pixel(120, 180);
        assert_eq!(
            viewport_pixel.0[3],
            u8::MAX,
            "central browser viewport should be opaque"
        );
        assert!(
            viewport_pixel.0[0] > 245 && viewport_pixel.0[1] > 245 && viewport_pixel.0[2] > 245,
            "central browser viewport should render Slate home background instead of black"
        );
    }

    #[test]
    fn headless_verification_regions_cover_known_chrome_assets() {
        let regions = verification_regions();
        let names = regions.iter().map(|region| region.name).collect::<Vec<_>>();

        for expected in [
            "rail-home-button",
            "rail-web-button",
            "rail-web-tab-previews",
            "rail-web-tab-close-buttons",
            "expanded-rail-web-button",
            "expanded-rail-web-tab-previews",
            "rail-home-icon",
            "rail-web-icon",
            "nav-back-icon",
            "nav-reload-icon",
            "nav-stop-icon",
            "address-security-icon",
            "toolbar-menu",
        ] {
            assert!(
                names.contains(&expected),
                "missing verification region {expected}"
            );
        }

        for region in regions {
            assert!(
                region.rect.left() >= 0.0
                    && region.rect.top() >= 0.0
                    && region.rect.right() <= DEFAULT_SNAPSHOT_WIDTH as f32
                    && region.rect.bottom() <= DEFAULT_SNAPSHOT_HEIGHT as f32,
                "verification region should stay within the canonical viewport: {region:?}"
            );
        }
    }

    #[test]
    fn headless_verification_metrics_track_synthetic_icon_detail() {
        let mut crop = image::RgbaImage::from_pixel(24, 24, image::Rgba([250, 250, 250, u8::MAX]));
        for y in 2..22 {
            crop.put_pixel(11, y, image::Rgba([30, 30, 30, u8::MAX]));
        }
        for x in 6..18 {
            crop.put_pixel(x, 12, image::Rgba([30, 30, 30, u8::MAX]));
        }

        let metrics = crop_metrics(&crop);

        assert_eq!(metrics.total_pixels, 24 * 24);
        assert_eq!(metrics.opaque_pixels, 24 * 24);
        assert!(metrics.detail_pixels > 20);
        assert!(metrics.dark_pixels > 20);
        assert!(
            metrics.left_edge_dark_pixels < 24,
            "ordinary glyph detail should stay below the selected-edge affordance threshold"
        );
        assert_eq!(metrics.vertical_detail_columns, 1);
        assert_eq!(
            metrics
                .detail_bounds
                .expect("detail bounds should exist")
                .min_x,
            6
        );
    }

    #[test]
    fn headless_verification_metrics_track_left_edge_affordance() {
        let mut crop = image::RgbaImage::from_pixel(24, 24, image::Rgba([250, 250, 250, u8::MAX]));
        for y in 4..20 {
            for x in 0..4 {
                crop.put_pixel(x, y, image::Rgba([20, 120, 116, u8::MAX]));
            }
        }

        let metrics = crop_metrics(&crop);

        assert_eq!(metrics.left_edge_dark_pixels, 64);
        assert!(metrics.detail_pixels >= 64);
    }

    #[test]
    fn headless_automated_review_flags_weak_selected_rail_affordance() {
        let mut regions = HashMap::new();
        regions.insert(
            "rail-web-button",
            RegionCapture {
                rect: PixelRect {
                    min_x: 0,
                    min_y: 0,
                    max_x: 72,
                    max_y: 73,
                },
                metrics: CropMetrics {
                    total_pixels: 72 * 73,
                    opaque_pixels: 72 * 73,
                    detail_pixels: 120,
                    dark_pixels: 20,
                    left_edge_dark_pixels: 0,
                    ..Default::default()
                },
            },
        );

        let review = automated_review_json(&regions);

        assert_eq!(review["summary"]["warnings"], 1);
        assert_eq!(review["findings"][0]["region"], "rail-web-button");
    }

    #[test]
    fn headless_automated_review_flags_offset_navigation_hover_shade() {
        let mut regions = HashMap::new();
        regions.insert(
            "nav-back-icon",
            RegionCapture {
                rect: PixelRect {
                    min_x: 38,
                    min_y: 4,
                    max_x: 70,
                    max_y: 36,
                },
                metrics: CropMetrics::default(),
            },
        );
        regions.insert(
            "nav-back-hover-button",
            RegionCapture {
                rect: PixelRect {
                    min_x: 0,
                    min_y: 0,
                    max_x: 44,
                    max_y: 44,
                },
                metrics: CropMetrics::default(),
            },
        );

        let findings = nav_hover_alignment_findings(&regions);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["region"], "nav-back-hover-button");
    }

    #[test]
    fn headless_region_monitor_flags_blank_ui_regions() {
        let monitor = RegionMonitor::new(4, 2, 2);
        let metrics = CropMetrics {
            total_pixels: 16,
            opaque_pixels: 16,
            ..Default::default()
        };

        let evaluation = evaluate_region_monitor(
            monitor,
            PixelRect {
                min_x: 0,
                min_y: 0,
                max_x: 4,
                max_y: 4,
            },
            metrics,
        );

        assert_eq!(evaluation.status, MonitorStatus::Fail);
        assert!(
            evaluation
                .failures
                .iter()
                .any(|failure| failure.contains("detail_pixels")),
            "blank region failure should mention missing detail"
        );
    }

    #[test]
    fn headless_region_monitor_warns_about_separator_like_detail() {
        let monitor = RegionMonitor::new(8, 4, 4)
            .with_dark_pixels(1)
            .with_vertical_detail(Some(1), Some(3));
        let metrics = CropMetrics {
            total_pixels: 24 * 24,
            opaque_pixels: 24 * 24,
            detail_pixels: 32,
            dark_pixels: 8,
            vertical_detail_columns: 4,
            detail_bounds: Some(PixelRect {
                min_x: 6,
                min_y: 4,
                max_x: 18,
                max_y: 20,
            }),
            ..Default::default()
        };

        let evaluation = evaluate_region_monitor(
            monitor,
            PixelRect {
                min_x: 0,
                min_y: 0,
                max_x: 24,
                max_y: 24,
            },
            metrics,
        );

        assert_eq!(evaluation.status, MonitorStatus::Warn);
        assert!(evaluation.failures.is_empty());
        assert_eq!(evaluation.warnings.len(), 1);
    }

    #[test]
    #[ignore = "renders the full canonical chrome fixture; use the CLI for routine visual verification"]
    fn headless_verification_report_writes_crops_and_metadata() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let output_dir = std::env::temp_dir().join(format!(
            "slate-chrome-verification-{}-{timestamp}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&output_dir);

        write_default_verification_report(&output_dir)
            .expect("headless verification report should render");

        let report = fs::read_to_string(output_dir.join("report.json"))
            .expect("verification report should be readable");
        assert!(report.contains("slate.chrome.visual-verification.v1"));
        assert!(report.contains("slate.chrome.automated-review.v1"));
        assert!(report.contains("\"rail-web-button\""));
        assert!(report.contains("\"nav-back-hover-button\""));
        assert!(report.contains("\"nav-reload-hover-button\""));
        assert!(report.contains("\"nav-stop-icon\""));
        assert!(output_dir.join("full.png").is_file());
        assert!(output_dir.join("loading-full.png").is_file());
        assert!(output_dir.join("hover-nav-back-full.png").is_file());
        assert!(output_dir.join("hover-nav-reload-full.png").is_file());

        let stop_crop = image::open(output_dir.join("nav-stop-icon.png"))
            .expect("Stop navigation crop should be a PNG")
            .into_rgba8();
        assert!(stop_crop.width() >= 24);
        assert!(stop_crop.height() >= 24);
        assert!(
            stop_crop
                .pixels()
                .any(|pixel| pixel.0[0] < 245 || pixel.0[1] < 245 || pixel.0[2] < 245),
            "Stop crop should contain visible raster detail"
        );

        let _ = fs::remove_dir_all(&output_dir);
    }
}
