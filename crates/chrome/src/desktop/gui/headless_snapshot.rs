/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::path::Path;

use egui::epaint::{ImageData, Primitive, Vertex};
use egui::{ClippedPrimitive, Color32, TextureId};
use image::RgbaImage;
use servo::LoadStatus;
use slate_broadwebd::BroadwebStatusSnapshot;

use super::{
    ADDRESS_BOOKMARK_ICON_SIZE, ADDRESS_BOOKMARK_RESERVED_WIDTH, ADDRESS_CORNER_RADIUS,
    ADDRESS_HEIGHT, ADDRESS_ICON_GAP, ADDRESS_INNER_MARGIN_X, ADDRESS_INPUT_TEXT_SIZE,
    ADDRESS_LEADING_GAP, ADDRESS_SECURITY_ICON_SIZE, ADDRESS_TEXT_HEIGHT, ADDRESS_TRAILING_GAP,
    APP_RAIL_WIDTH, AddressSecurityIcon, FOOTER_HEIGHT, Gui, NEW_TAB_BUTTON_SIZE, NEW_TAB_LEFT_GAP,
    NEW_TAB_SLOT_HEIGHT, RAIL_PANEL_MARGIN_X, RAIL_PANEL_MARGIN_Y, TAB_CONTENT_ALIGN,
    TAB_CONTENT_HEIGHT, TAB_ICON_SIZE, TAB_ICON_TITLE_GAP, TAB_INNER_MARGIN_X, TAB_INNER_MARGIN_Y,
    TAB_STRIP_HEIGHT, TAB_TITLE_CLOSE_GAP, TOOLBAR_HEIGHT, TOOLBAR_ITEM_SPACING,
    TOOLBAR_PANEL_MARGIN_X, TOOLBAR_PANEL_MARGIN_Y, address_background_color,
    address_bookmark_icon_color, address_border_color, address_security_icon_for_location,
    address_security_raster_color, address_slate_security_icon_rect, chrome_panel_background_color,
    configure_fonts, default_home_bookmark_cards, draw_inactive_tab_outline,
    draw_tab_strip_separator, footer_panel_margin, home_view_background_color,
    inactive_tab_background_color, inactive_tab_hover_background_color, slate_theme,
    tab_close_icon_color, tab_close_raster, tab_content_width, tab_corner_radius, tab_icon_color,
    tab_strip_background_color, tab_width_for_strip, toolbar_address_width,
    toolbar_background_color,
};
use crate::desktop::slate_theme::{SlateIcon, SlateIconCache, SlateRaster};

pub(crate) const DEFAULT_SNAPSHOT_WIDTH: u32 = 1672;
pub(crate) const DEFAULT_SNAPSHOT_HEIGHT: u32 = 941;

pub(crate) fn write_default_snapshot(path: &Path) -> Result<(), String> {
    write_snapshot(path, [DEFAULT_SNAPSHOT_WIDTH, DEFAULT_SNAPSHOT_HEIGHT])
}

pub(crate) fn write_snapshot(path: &Path, size: [u32; 2]) -> Result<(), String> {
    let image = render_snapshot(size)?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    image
        .save(path)
        .map_err(|error| format!("failed to encode PNG: {error}"))
}

fn render_snapshot(size: [u32; 2]) -> Result<RgbaImage, String> {
    let ctx = egui::Context::default();
    ctx.set_fonts(configure_fonts());
    ctx.options_mut(|options| {
        options.zoom_with_keyboard = false;
        options.fallback_theme = egui::Theme::Light;
    });
    slate_theme::apply(&ctx);

    let screen_rect =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32));
    let mut input = egui::RawInput {
        screen_rect: Some(screen_rect),
        focused: true,
        time: Some(0.0),
        ..Default::default()
    };
    if let Some(viewport) = input.viewports.get_mut(&egui::ViewportId::ROOT) {
        viewport.native_pixels_per_point = Some(1.0);
        viewport.inner_rect = Some(screen_rect);
    }

    let mut slate_icons = SlateIconCache::default();
    let mut location = "slate://home".to_owned();
    let mut home_search = String::new();
    let output = ctx.run_ui(input, |ui| {
        render_chrome_fixture(ui, &mut slate_icons, &mut location, &mut home_search);
    });

    let mut renderer = SoftwareRenderer::new(size);
    renderer.apply_textures_delta(&output.textures_delta)?;
    let pixels_per_point = output.pixels_per_point;
    let clipped_primitives = ctx.tessellate(output.shapes, pixels_per_point);
    renderer.paint(&clipped_primitives, pixels_per_point)?;
    Ok(renderer.into_image())
}

fn render_chrome_fixture(
    root_ui: &mut egui::Ui,
    slate_icons: &mut SlateIconCache,
    location: &mut String,
    home_search: &mut String,
) {
    render_tab_strip(root_ui, slate_icons);
    render_app_rail(root_ui, slate_icons);
    render_toolbar(root_ui, slate_icons, location);
    let footer_rect = render_footer(root_ui);

    render_home_panel(root_ui, slate_icons, home_search);
    Gui::draw_footer_top_separator(root_ui.ctx(), footer_rect);
}

fn render_tab_strip(root_ui: &mut egui::Ui, slate_icons: &mut SlateIconCache) {
    let tabs_frame = egui::Frame::NONE
        .fill(tab_strip_background_color())
        .inner_margin(egui::Margin::symmetric(0, 0));
    egui::Panel::top("headless_tabs")
        .exact_size(TAB_STRIP_HEIGHT)
        .frame(tabs_frame)
        .show_separator_line(false)
        .show_inside(root_ui, |ui| {
            let mut active_tab_rect = None;
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    Gui::draw_app_title(ui);
                    let tab_strip_available_width = ui.available_width();
                    egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(
                                ui.available_size(),
                                egui::Layout::left_to_right(super::TAB_STRIP_CONTENT_ALIGN),
                                |ui| {
                                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                                    let tabs = [
                                        ("New Tab", true),
                                        ("Privacy Dashboard", false),
                                        ("Calendar", false),
                                    ];
                                    let tab_width =
                                        tab_width_for_strip(tab_strip_available_width, tabs.len());
                                    for (index, (label, active)) in tabs.iter().enumerate() {
                                        let tab_rect = draw_snapshot_tab(
                                            ui,
                                            slate_icons,
                                            index,
                                            label,
                                            *active,
                                            tab_width,
                                        );
                                        if *active {
                                            active_tab_rect = Some(tab_rect);
                                        }
                                    }

                                    ui.add_space(NEW_TAB_LEFT_GAP);
                                    let _ = ui
                                        .allocate_ui_with_layout(
                                            egui::vec2(NEW_TAB_BUTTON_SIZE, NEW_TAB_SLOT_HEIGHT),
                                            egui::Layout::left_to_right(TAB_CONTENT_ALIGN),
                                            Gui::new_tab_button,
                                        )
                                        .inner;
                                },
                            );
                        });
                },
            );
            draw_tab_strip_separator(ui, active_tab_rect);
        });
}

fn draw_snapshot_tab(
    ui: &mut egui::Ui,
    slate_icons: &mut SlateIconCache,
    index: usize,
    label: &str,
    active: bool,
    tab_width: f32,
) -> egui::Rect {
    let inactive_bg_color = inactive_tab_background_color();
    let inactive_hover_bg_color = inactive_tab_hover_background_color();
    let active_bg_color = super::active_tab_background_color();
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
    let fallback_icon = slate_icons.texture(
        ui.ctx(),
        Gui::fallback_tab_icon(index),
        tab_icon_color(active),
    );
    let close_icon = slate_icons.raster_mask_texture(
        ui.ctx(),
        tab_close_raster(active),
        tab_close_icon_color(active),
    );

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
        visuals.widgets.active.bg_stroke.width = 0.0;
        visuals.widgets.hovered.bg_stroke.width = 0.0;
        visuals.widgets.noninteractive.weak_bg_fill = tab_content_bg_color;
        visuals.widgets.inactive.weak_bg_fill = tab_content_bg_color;
        visuals.widgets.hovered.weak_bg_fill = tab_content_hover_bg_color;
        visuals.widgets.active.weak_bg_fill = tab_content_hover_bg_color;
        visuals.selection.bg_fill = active_bg_color;
        visuals.selection.stroke.color = visuals.widgets.active.fg_stroke.color;
        visuals.widgets.hovered.fg_stroke.color = visuals.widgets.active.fg_stroke.color;
        visuals.widgets.active.expansion = 0.0;
        visuals.widgets.hovered.expansion = 0.0;

        tab_frame.content_ui.allocate_ui_with_layout(
            egui::vec2(tab_content_width, TAB_CONTENT_HEIGHT),
            egui::Layout::left_to_right(TAB_CONTENT_ALIGN),
            |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.add(Gui::icon_image(fallback_icon, TAB_ICON_SIZE));
                ui.add_space(TAB_ICON_TITLE_GAP);
                let _ = Gui::tab_title_button(ui, label, active, tab_content_width);
                ui.add_space(TAB_TITLE_CLOSE_GAP);
                let _ = Gui::tab_close_button(ui, close_icon);
            },
        );
    }

    let response = tab_frame.allocate_space(ui);
    tab_frame.frame.fill = if active {
        active_bg_color
    } else if response.hovered() {
        inactive_hover_bg_color
    } else {
        inactive_bg_color
    };
    tab_frame.end(ui);
    if !active {
        draw_inactive_tab_outline(ui, response.rect);
    }
    response.rect
}

fn render_app_rail(root_ui: &mut egui::Ui, slate_icons: &mut SlateIconCache) {
    let rail_frame = egui::Frame::NONE
        .fill(chrome_panel_background_color())
        .inner_margin(egui::Margin::symmetric(
            RAIL_PANEL_MARGIN_X,
            RAIL_PANEL_MARGIN_Y,
        ));
    egui::Panel::left("headless_app_rail")
        .exact_size(APP_RAIL_WIDTH)
        .frame(rail_frame)
        .show_separator_line(true)
        .show_inside(root_ui, |ui| Gui::draw_app_rail(ui, slate_icons));
}

fn render_toolbar(root_ui: &mut egui::Ui, slate_icons: &mut SlateIconCache, location: &mut String) {
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
                    let _ =
                        Gui::toolbar_navigation_button(ui, slate_icons, SlateIcon::NavBack, false);
                    let _ = Gui::toolbar_navigation_button(
                        ui,
                        slate_icons,
                        SlateIcon::NavForward,
                        false,
                    );
                    let _ = Gui::toolbar_navigation_button(
                        ui,
                        slate_icons,
                        SlateIcon::NavRefresh,
                        true,
                    );

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
        .stroke(egui::Stroke::new(1.0, address_border_color()))
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

fn render_footer(root_ui: &mut egui::Ui) -> egui::Rect {
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
                LoadStatus::Complete,
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
                        home_search,
                        &default_home_bookmark_cards(),
                    )
                })
                .inner;
            let _ = response.layout;
            let _ = response.navigation_request;
        });
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
    use super::{DEFAULT_SNAPSHOT_HEIGHT, DEFAULT_SNAPSHOT_WIDTH, render_snapshot};

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
}
