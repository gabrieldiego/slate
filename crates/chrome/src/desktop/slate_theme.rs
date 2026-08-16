/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use egui::{Color32, TextureHandle, TextureOptions};

pub(crate) const BG: Color32 = Color32::from_rgb(249, 248, 247);
pub(crate) const CHROME_BG: Color32 = BG;
pub(crate) const HOME_BG: Color32 = Color32::from_rgb(251, 250, 250);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub(crate) const FIELD_SURFACE: Color32 = Color32::from_rgb(254, 253, 252);
pub(crate) const TITLE_SURFACE: Color32 = Color32::from_rgb(247, 246, 245);
pub(crate) const PANEL: Color32 = Color32::from_rgb(238, 235, 232);
pub(crate) const PANEL_HOVER: Color32 = Color32::from_rgb(236, 235, 233);
pub(crate) const BORDER: Color32 = Color32::from_rgb(229, 226, 225);
pub(crate) const FIELD_BORDER: Color32 = Color32::from_rgb(232, 232, 233);
pub(crate) const TEXT: Color32 = Color32::from_rgb(43, 45, 45);
pub(crate) const MUTED: Color32 = Color32::from_rgb(120, 120, 121);
pub(crate) const TEAL: Color32 = Color32::from_rgb(11, 95, 95);
pub(crate) const TEAL_SOFT: Color32 = Color32::from_rgb(238, 243, 243);
pub(crate) const AMBER: Color32 = Color32::from_rgb(216, 147, 12);
pub(crate) const BLUE: Color32 = Color32::from_rgb(9, 109, 207);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SlateIcon {
    AppCalendar,
    AppDownloads,
    AppHome,
    AppMessaging,
    AppWeb,
    HomeFooterShield,
    HomeHeroShield,
    HomeSearch,
    HomeMetricAds,
    HomeMetricLock,
    HomeMetricPrivacy,
    HomeMetricTime,
    NavBack,
    NavForward,
    NavRefresh,
    TabCalendar,
    TabResearch,
    TabWeb,
    TopShield,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SlateRaster {
    BookmarkAdd,
    NavBack,
    NavBackHover,
    NavForward,
    NavForwardHover,
    NavReload,
    NavReloadHover,
    NavStop,
    NavStopHover,
    PageInfoInsecure,
    PageInfoInternal,
    PageInfoLocal,
    PageInfoSecure,
    PageInfoWarning,
    Search,
    TabClose,
    #[allow(dead_code)]
    TabCloseMuted,
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
            Self::AppHome => SlateIconData {
                name: "app-home",
                width: 34,
                height: 34,
                mask: include_bytes!("../../assets/icons/home.alpha"),
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
            Self::HomeSearch => SlateIconData {
                name: "home-search",
                width: 32,
                height: 32,
                mask: include_bytes!("../../assets/icons/home_search.alpha"),
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
            Self::NavBack => SlateIconData {
                name: "nav-back",
                width: 32,
                height: 32,
                mask: include_bytes!("../../assets/icons/nav_back.alpha"),
            },
            Self::NavForward => SlateIconData {
                name: "nav-forward",
                width: 32,
                height: 32,
                mask: include_bytes!("../../assets/icons/nav_forward.alpha"),
            },
            Self::NavRefresh => SlateIconData {
                name: "nav-refresh",
                width: 32,
                height: 32,
                mask: include_bytes!("../../assets/icons/nav_refresh.alpha"),
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
                width: 28,
                height: 28,
                bytes: include_bytes!("../../assets/icons/concept/nav_back.png"),
            },
            Self::NavBackHover => SlateRasterData {
                name: "nav-back-hover",
                width: 28,
                height: 28,
                bytes: include_bytes!("../../assets/icons/concept/nav_back.png"),
            },
            Self::NavForward => SlateRasterData {
                name: "nav-forward",
                width: 28,
                height: 28,
                bytes: include_bytes!("../../assets/icons/concept/nav_forward.png"),
            },
            Self::NavForwardHover => SlateRasterData {
                name: "nav-forward-hover",
                width: 28,
                height: 28,
                bytes: include_bytes!("../../assets/icons/concept/nav_forward.png"),
            },
            Self::NavReload => SlateRasterData {
                name: "nav-reload",
                width: 28,
                height: 28,
                bytes: include_bytes!("../../assets/icons/concept/nav_reload.png"),
            },
            Self::NavReloadHover => SlateRasterData {
                name: "nav-reload-hover",
                width: 28,
                height: 28,
                bytes: include_bytes!("../../assets/icons/concept/nav_reload.png"),
            },
            Self::NavStop => SlateRasterData {
                name: "nav-stop",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/stop.png"),
            },
            Self::NavStopHover => SlateRasterData {
                name: "nav-stop-hover",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/stop_h.png"),
            },
            Self::PageInfoInsecure => SlateRasterData {
                name: "page-info-insecure",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/page-info-insecure.png"),
            },
            Self::PageInfoInternal => SlateRasterData {
                name: "page-info-internal",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/page-info-internal.png"),
            },
            Self::PageInfoLocal => SlateRasterData {
                name: "page-info-local",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/page-info-local.png"),
            },
            Self::PageInfoSecure => SlateRasterData {
                name: "page-info-secure",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/page-info-secure.png"),
            },
            Self::PageInfoWarning => SlateRasterData {
                name: "page-info-warning",
                width: 24,
                height: 24,
                bytes: include_bytes!("../../assets/icons/slate-ns/page-info-warning.png"),
            },
            Self::Search => SlateRasterData {
                name: "search",
                width: 17,
                height: 17,
                bytes: include_bytes!("../../assets/icons/slate-ns/search.png"),
            },
            Self::TabClose => SlateRasterData {
                name: "tab-close",
                width: 12,
                height: 12,
                bytes: include_bytes!("../../assets/icons/slate-ns/closetab.png"),
            },
            Self::TabCloseMuted => SlateRasterData {
                name: "tab-close-muted",
                width: 12,
                height: 12,
                bytes: include_bytes!("../../assets/icons/slate-ns/closetab_g.png"),
            },
        }
    }
}

#[derive(Default)]
pub(crate) struct SlateIconCache {
    textures: HashMap<(SlateIcon, [u8; 4]), TextureHandle>,
    raster_mask_textures: HashMap<(SlateRaster, [u8; 4]), TextureHandle>,
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

    pub(crate) fn raster_mask_texture(
        &mut self,
        ctx: &egui::Context,
        raster: SlateRaster,
        color: Color32,
    ) -> egui::load::SizedTexture {
        let color_key = color.to_array();
        let handle = self
            .raster_mask_textures
            .entry((raster, color_key))
            .or_insert_with(|| load_raster_mask_texture(ctx, raster, color));
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
    visuals.widgets.noninteractive.bg_stroke.color = BORDER;
    visuals.widgets.inactive.bg_stroke.color = BORDER;
    visuals.widgets.hovered.bg_stroke.color = BORDER;
    visuals.widgets.active.bg_stroke.color = TEAL;
    visuals.widgets.hovered.expansion = 0.0;
    visuals.widgets.active.expansion = 0.0;
    visuals.weak_text_color = Some(MUTED);
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

fn raster_mask_rgba(raster: SlateRaster, color: Color32) -> Vec<u8> {
    let data = raster.data();
    let image = image::load_from_memory(data.bytes)
        .expect("bundled Slate PNG asset should decode")
        .to_rgba8();
    debug_assert_eq!(image.width() as usize, data.width);
    debug_assert_eq!(image.height() as usize, data.height);

    let [red, green, blue, color_alpha] = color.to_array();
    let has_source_alpha = image.pixels().any(|pixel| pixel.0[3] < u8::MAX);
    let opaque_background = image
        .pixels()
        .map(|pixel| pixel.0[0].min(pixel.0[1]).min(pixel.0[2]))
        .max()
        .unwrap_or(u8::MAX)
        .max(1);
    image
        .pixels()
        .flat_map(|pixel| {
            let source_alpha = if has_source_alpha {
                pixel.0[3]
            } else {
                let brightness = pixel.0[0].min(pixel.0[1]).min(pixel.0[2]);
                let distance = opaque_background.saturating_sub(brightness);
                u8::try_from(
                    u16::from(distance) * u16::from(u8::MAX) / u16::from(opaque_background),
                )
                .unwrap_or(u8::MAX)
            };
            let alpha = u16::from(source_alpha) * u16::from(color_alpha) / 255;
            [red, green, blue, u8::try_from(alpha).unwrap_or(u8::MAX)]
        })
        .collect()
}

fn load_raster_mask_texture(
    ctx: &egui::Context,
    raster: SlateRaster,
    color: Color32,
) -> TextureHandle {
    let data = raster.data();
    let [red, green, blue, color_alpha] = color.to_array();
    let rgba = raster_mask_rgba(raster, color);
    let image = egui::ColorImage::from_rgba_unmultiplied([data.width, data.height], &rgba);
    ctx.load_texture(
        format!(
            "slate-raster-mask-{}-{:02x}{:02x}{:02x}{:02x}",
            data.name, red, green, blue, color_alpha
        ),
        image,
        TextureOptions::LINEAR,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        AMBER, BG, BLUE, BORDER, CHROME_BG, FIELD_BORDER, FIELD_SURFACE, HOME_BG, MUTED, PANEL,
        PANEL_HOVER, SlateIcon, SlateRaster, TEAL, TEAL_SOFT, TITLE_SURFACE, raster_mask_rgba,
    };

    #[test]
    fn bundled_alpha_masks_match_declared_dimensions() {
        for icon in [
            SlateIcon::AppCalendar,
            SlateIcon::AppDownloads,
            SlateIcon::AppHome,
            SlateIcon::AppMessaging,
            SlateIcon::AppWeb,
            SlateIcon::HomeFooterShield,
            SlateIcon::HomeHeroShield,
            SlateIcon::HomeSearch,
            SlateIcon::HomeMetricAds,
            SlateIcon::HomeMetricLock,
            SlateIcon::HomeMetricPrivacy,
            SlateIcon::HomeMetricTime,
            SlateIcon::NavBack,
            SlateIcon::NavForward,
            SlateIcon::NavRefresh,
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
            SlateRaster::NavBackHover,
            SlateRaster::NavForward,
            SlateRaster::NavForwardHover,
            SlateRaster::NavReload,
            SlateRaster::NavReloadHover,
            SlateRaster::NavStop,
            SlateRaster::NavStopHover,
            SlateRaster::PageInfoInsecure,
            SlateRaster::PageInfoInternal,
            SlateRaster::PageInfoLocal,
            SlateRaster::PageInfoSecure,
            SlateRaster::PageInfoWarning,
            SlateRaster::Search,
            SlateRaster::TabClose,
            SlateRaster::TabCloseMuted,
        ] {
            let data = raster.data();
            let image = image::load_from_memory(data.bytes).unwrap().to_rgba8();
            assert_eq!(image.width() as usize, data.width);
            assert_eq!(image.height() as usize, data.height);
        }
    }

    #[test]
    fn raster_mask_uses_source_alpha_with_requested_color() {
        let rgba = raster_mask_rgba(SlateRaster::BookmarkAdd, MUTED);
        let data = SlateRaster::BookmarkAdd.data();
        let [red, green, blue, _] = MUTED.to_array();

        assert_eq!(rgba.len(), data.width * data.height * 4);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert!(
            rgba.chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .all(|pixel| pixel[0] == red && pixel[1] == green && pixel[2] == blue)
        );
    }

    #[test]
    fn raster_mask_derives_alpha_for_white_backed_legacy_assets() {
        let rgba = raster_mask_rgba(SlateRaster::TabClose, MUTED);
        let data = SlateRaster::TabClose.data();
        let [red, green, blue, _] = MUTED.to_array();
        let mut alpha_values = rgba.chunks_exact(4).map(|pixel| pixel[3]);

        assert_eq!(rgba.len(), data.width * data.height * 4);
        assert_eq!(alpha_values.clone().min(), Some(0));
        assert!(alpha_values.any(|alpha| alpha > 200));
        assert!(
            rgba.chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .all(|pixel| pixel[0] == red && pixel[1] == green && pixel[2] == blue)
        );
    }

    #[test]
    fn palette_uses_warm_concept_layers() {
        assert_eq!(BG, egui::Color32::from_rgb(249, 248, 247));
        assert_eq!(CHROME_BG, egui::Color32::from_rgb(249, 248, 247));
        assert_eq!(HOME_BG, egui::Color32::from_rgb(251, 250, 250));
        assert_eq!(FIELD_SURFACE, egui::Color32::from_rgb(254, 253, 252));
        assert_eq!(TITLE_SURFACE, egui::Color32::from_rgb(247, 246, 245));
        assert_eq!(PANEL, egui::Color32::from_rgb(238, 235, 232));
        assert_eq!(PANEL_HOVER, egui::Color32::from_rgb(236, 235, 233));
        assert_eq!(BORDER, egui::Color32::from_rgb(229, 226, 225));
        assert_eq!(FIELD_BORDER, egui::Color32::from_rgb(232, 232, 233));
        assert_eq!(MUTED, egui::Color32::from_rgb(120, 120, 121));
        assert_eq!(TEAL, egui::Color32::from_rgb(11, 95, 95));
        assert_eq!(TEAL_SOFT, egui::Color32::from_rgb(238, 243, 243));
        assert_eq!(AMBER, egui::Color32::from_rgb(216, 147, 12));
        assert_eq!(BLUE, egui::Color32::from_rgb(9, 109, 207));
    }

    #[test]
    fn theme_uses_muted_weak_text_for_placeholders() {
        let ctx = egui::Context::default();

        super::apply(&ctx);

        assert_eq!(ctx.global_style().visuals.weak_text_color(), MUTED);
    }
}
