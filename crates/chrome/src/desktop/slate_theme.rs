/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use egui::{Color32, TextureHandle, TextureOptions};

pub(crate) const BG: Color32 = Color32::from_rgb(251, 250, 248);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub(crate) const PANEL: Color32 = Color32::from_rgb(244, 242, 239);
pub(crate) const PANEL_HOVER: Color32 = Color32::from_rgb(239, 237, 233);
pub(crate) const BORDER: Color32 = Color32::from_rgb(221, 217, 212);
pub(crate) const TEXT: Color32 = Color32::from_rgb(39, 39, 39);
pub(crate) const MUTED: Color32 = Color32::from_rgb(111, 107, 103);
pub(crate) const TEAL: Color32 = Color32::from_rgb(11, 107, 104);
pub(crate) const TEAL_SOFT: Color32 = Color32::from_rgb(229, 240, 238);
pub(crate) const AMBER: Color32 = Color32::from_rgb(217, 154, 0);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SlateIcon {
    AppCalendar,
    AppDownloads,
    AppMessaging,
    AppWeb,
    HomeFooterShield,
    HomeHeroShield,
    HomeMetricAds,
    HomeMetricLock,
    HomeMetricPrivacy,
    HomeMetricTime,
    HomeSearch,
    TabCalendar,
    TabResearch,
    TabWeb,
    TopShield,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SlateRaster {
    BookmarkAdd,
    NavBack,
    NavBackDisabled,
    NavForward,
    NavForwardDisabled,
    NavRefresh,
    NavStop,
    PageInfoSecure,
}

#[derive(Clone, Copy, Debug)]
struct SlateIconData {
    name: &'static str,
    width: usize,
    height: usize,
    mask: &'static [u8],
}

#[derive(Clone, Copy, Debug)]
struct SlateRasterData {
    name: &'static str,
    width: usize,
    height: usize,
    bytes: &'static [u8],
}

impl SlateIcon {
    fn data(self) -> SlateIconData {
        match self {
            Self::AppCalendar => SlateIconData {
                name: "app-calendar",
                width: 34,
                height: 34,
                mask: include_bytes!("../../assets/icons/calendar.alpha"),
            },
            Self::AppDownloads => SlateIconData {
                name: "app-downloads",
                width: 34,
                height: 34,
                mask: include_bytes!("../../assets/icons/downloads.alpha"),
            },
            Self::AppMessaging => SlateIconData {
                name: "app-messaging",
                width: 34,
                height: 34,
                mask: include_bytes!("../../assets/icons/messaging.alpha"),
            },
            Self::AppWeb => SlateIconData {
                name: "app-web",
                width: 34,
                height: 34,
                mask: include_bytes!("../../assets/icons/web.alpha"),
            },
            Self::HomeFooterShield => SlateIconData {
                name: "home-footer-shield",
                width: 28,
                height: 28,
                mask: include_bytes!("../../assets/icons/home_footer_shield.alpha"),
            },
            Self::HomeHeroShield => SlateIconData {
                name: "home-hero-shield",
                width: 64,
                height: 64,
                mask: include_bytes!("../../assets/icons/home_hero_shield.alpha"),
            },
            Self::HomeMetricAds => SlateIconData {
                name: "home-metric-ads",
                width: 40,
                height: 40,
                mask: include_bytes!("../../assets/icons/home_metric_ads.alpha"),
            },
            Self::HomeMetricLock => SlateIconData {
                name: "home-metric-lock",
                width: 40,
                height: 40,
                mask: include_bytes!("../../assets/icons/home_metric_lock.alpha"),
            },
            Self::HomeMetricPrivacy => SlateIconData {
                name: "home-metric-privacy",
                width: 40,
                height: 40,
                mask: include_bytes!("../../assets/icons/home_metric_privacy.alpha"),
            },
            Self::HomeMetricTime => SlateIconData {
                name: "home-metric-time",
                width: 40,
                height: 40,
                mask: include_bytes!("../../assets/icons/home_metric_time.alpha"),
            },
            Self::HomeSearch => SlateIconData {
                name: "home-search",
                width: 32,
                height: 32,
                mask: include_bytes!("../../assets/icons/home_search.alpha"),
            },
            Self::TabCalendar => SlateIconData {
                name: "tab-calendar",
                width: 20,
                height: 20,
                mask: include_bytes!("../../assets/icons/tab_calendar.alpha"),
            },
            Self::TabResearch => SlateIconData {
                name: "tab-research",
                width: 20,
                height: 20,
                mask: include_bytes!("../../assets/icons/tab_research.alpha"),
            },
            Self::TabWeb => SlateIconData {
                name: "tab-web",
                width: 20,
                height: 20,
                mask: include_bytes!("../../assets/icons/tab_web.alpha"),
            },
            Self::TopShield => SlateIconData {
                name: "top-shield",
                width: 28,
                height: 28,
                mask: include_bytes!("../../assets/icons/top_shield.alpha"),
            },
        }
    }
}

impl SlateRaster {
    fn data(self) -> SlateRasterData {
        match self {
            Self::BookmarkAdd => SlateRasterData {
                name: "bookmark-add",
                width: 17,
                height: 17,
                bytes: include_bytes!("../../assets/icons/slate-ns/hotlist-add.png"),
            },
            Self::NavBack => SlateRasterData {
                name: "nav-back",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/back.png"),
            },
            Self::NavBackDisabled => SlateRasterData {
                name: "nav-back-disabled",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/back_g.png"),
            },
            Self::NavForward => SlateRasterData {
                name: "nav-forward",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/forward.png"),
            },
            Self::NavForwardDisabled => SlateRasterData {
                name: "nav-forward-disabled",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/forward_g.png"),
            },
            Self::NavRefresh => SlateRasterData {
                name: "nav-refresh",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/reload.png"),
            },
            Self::NavStop => SlateRasterData {
                name: "nav-stop",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/stop.png"),
            },
            Self::PageInfoSecure => SlateRasterData {
                name: "page-info-secure",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/page-info-secure.png"),
            },
        }
    }
}

#[derive(Default)]
pub(crate) struct SlateIconCache {
    textures: HashMap<(SlateIcon, [u8; 4]), TextureHandle>,
    raster_textures: HashMap<SlateRaster, TextureHandle>,
}

impl SlateIconCache {
    pub(crate) fn texture(
        &mut self,
        ctx: &egui::Context,
        icon: SlateIcon,
        color: Color32,
    ) -> egui::load::SizedTexture {
        let color_key = color.to_array();
        let handle = self
            .textures
            .entry((icon, color_key))
            .or_insert_with(|| load_icon_texture(ctx, icon, color));
        let data = icon.data();
        egui::load::SizedTexture::new(
            handle.id(),
            egui::vec2(data.width as f32, data.height as f32),
        )
    }

    pub(crate) fn raster_texture(
        &mut self,
        ctx: &egui::Context,
        raster: SlateRaster,
    ) -> egui::load::SizedTexture {
        let handle = self
            .raster_textures
            .entry(raster)
            .or_insert_with(|| load_raster_texture(ctx, raster));
        let data = raster.data();
        egui::load::SizedTexture::new(
            handle.id(),
            egui::vec2(data.width as f32, data.height as f32),
        )
    }
}

pub(crate) fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = SURFACE;
    visuals.panel_fill = BG;
    visuals.faint_bg_color = PANEL;
    visuals.extreme_bg_color = SURFACE;
    visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    visuals.widgets.inactive.fg_stroke.color = TEXT;
    visuals.widgets.hovered.fg_stroke.color = TEXT;
    visuals.widgets.active.fg_stroke.color = TEXT;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.weak_bg_fill = PANEL_HOVER;
    visuals.widgets.active.weak_bg_fill = PANEL;
    visuals.selection.bg_fill = TEAL_SOFT;
    visuals.selection.stroke.color = TEAL;
    ctx.set_visuals(visuals);
}

fn load_icon_texture(ctx: &egui::Context, icon: SlateIcon, color: Color32) -> TextureHandle {
    let data = icon.data();
    debug_assert_eq!(data.mask.len(), data.width.saturating_mul(data.height));

    let [red, green, blue, color_alpha] = color.to_array();
    let rgba: Vec<u8> = data
        .mask
        .iter()
        .flat_map(|alpha| {
            let alpha = u16::from(*alpha) * u16::from(color_alpha) / 255;
            [red, green, blue, u8::try_from(alpha).unwrap_or(u8::MAX)]
        })
        .collect();
    let image = egui::ColorImage::from_rgba_unmultiplied([data.width, data.height], &rgba);

    ctx.load_texture(
        format!(
            "slate-{}-{:02x}{:02x}{:02x}{:02x}",
            data.name, red, green, blue, color_alpha
        ),
        image,
        TextureOptions::LINEAR,
    )
}

fn load_raster_texture(ctx: &egui::Context, raster: SlateRaster) -> TextureHandle {
    let data = raster.data();
    let image = image::load_from_memory(data.bytes)
        .expect("bundled Slate PNG asset should decode")
        .to_rgba8();
    debug_assert_eq!(image.width() as usize, data.width);
    debug_assert_eq!(image.height() as usize, data.height);

    let image = egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    );
    ctx.load_texture(
        format!("slate-raster-{}", data.name),
        image,
        TextureOptions::LINEAR,
    )
}

#[cfg(test)]
mod tests {
    use super::{SlateIcon, SlateRaster};

    #[test]
    fn bundled_alpha_masks_match_declared_dimensions() {
        for icon in [
            SlateIcon::AppCalendar,
            SlateIcon::AppDownloads,
            SlateIcon::AppMessaging,
            SlateIcon::AppWeb,
            SlateIcon::HomeFooterShield,
            SlateIcon::HomeHeroShield,
            SlateIcon::HomeMetricAds,
            SlateIcon::HomeMetricLock,
            SlateIcon::HomeMetricPrivacy,
            SlateIcon::HomeMetricTime,
            SlateIcon::HomeSearch,
            SlateIcon::TabCalendar,
            SlateIcon::TabResearch,
            SlateIcon::TabWeb,
            SlateIcon::TopShield,
        ] {
            let data = icon.data();
            assert_eq!(data.mask.len(), data.width * data.height);
        }
    }

    #[test]
    fn bundled_raster_images_match_declared_dimensions() {
        for raster in [
            SlateRaster::BookmarkAdd,
            SlateRaster::NavBack,
            SlateRaster::NavBackDisabled,
            SlateRaster::NavForward,
            SlateRaster::NavForwardDisabled,
            SlateRaster::NavRefresh,
            SlateRaster::NavStop,
            SlateRaster::PageInfoSecure,
        ] {
            let data = raster.data();
            let image = image::load_from_memory(data.bytes).unwrap().to_rgba8();
            assert_eq!(image.width() as usize, data.width);
            assert_eq!(image.height() as usize, data.height);
        }
    }
}
