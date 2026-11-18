//! The Altrune pictures: the symbol beside the name in the sidebar and the full logo in
//! «Acerca de». The files are embedded in the program, decoded once (premultiplied, as egui
//! wants them), and each is cut down once to the exact pixels it covers on this screen with
//! Lanczos-3, so the edges are crisp at any scale instead of being squeezed by the graphics card
//! every frame. A texture is made when a picture is first drawn at a size and kept until the
//! picture is wanted at another size (the window moved to a screen with a different density).
//! Nothing here animates or starts a process.

use eframe::egui::{self, pos2, vec2, Color32, ColorImage, Rect, TextureHandle, TextureOptions};
use image::imageops::{self, FilterType};
use image::RgbaImage;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The pictures of the brand (`assets/branding/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Art {
    /// The «A» alone, for the sidebar.
    Symbol,
    /// The logo's symbol layer, in its own colours.
    LogoSymbol,
    /// The logo's wordmark as a white mask with alpha, to be tinted with the text colour. It
    /// shares its canvas with `LogoSymbol`: drawn in the same rectangle they recompose the logo.
    LogoWord,
}

impl Art {
    fn file(self) -> &'static [u8] {
        match self {
            Art::Symbol => include_bytes!("../../assets/branding/altrune-simbolo-128.png"),
            Art::LogoSymbol => {
                include_bytes!("../../assets/branding/altrune-logotipo-simbolo-512.png")
            }
            Art::LogoWord => {
                include_bytes!("../../assets/branding/altrune-logotipo-palabra-512.png")
            }
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The picture decoded, with its colours premultiplied by its alpha. Decoded the first time it is
/// asked for and never again.
fn decoded(art: Art) -> &'static RgbaImage {
    static CELLS: [OnceLock<RgbaImage>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    CELLS[art.index()].get_or_init(|| {
        // The files are part of the program, so a failure here is a build mistake; a clear
        // 1 x 1 picture keeps the window working anyway.
        let mut img = image::load_from_memory(art.file())
            .map(|i| i.to_rgba8())
            .unwrap_or_else(|_| RgbaImage::new(1, 1));
        for px in img.pixels_mut() {
            let a = u32::from(px.0[3]);
            for c in &mut px.0[..3] {
                *c = ((u32::from(*c) * a + 127) / 255) as u8;
            }
        }
        img
    })
}

/// The picture's size in pixels at its original resolution.
pub fn native_size(art: Art) -> [u32; 2] {
    let img = decoded(art);
    [img.width(), img.height()]
}

/// The size in pixels the picture is shown at when it is `width_px` wide: the same proportions,
/// never larger than the original (a larger one would only be a blurrier copy).
pub fn pixel_size(art: Art, width_px: u32) -> [u32; 2] {
    let [w, h] = native_size(art);
    let width = width_px.clamp(1, w);
    let height = (u64::from(h) * u64::from(width) + u64::from(w) / 2) / u64::from(w);
    [width, (height as u32).max(1)]
}

/// The picture cut down to `width_px` (premultiplied, ready for a texture).
pub fn scaled(art: Art, width_px: u32) -> ColorImage {
    let src = decoded(art);
    let [w, h] = pixel_size(art, width_px);
    let resized;
    let img = if (w, h) == (src.width(), src.height()) {
        src
    } else {
        resized = imageops::resize(src, w, h, FilterType::Lanczos3);
        &resized
    };
    let pixels = img
        .pixels()
        .map(|px| {
            // The filter can overshoot a little: a premultiplied colour never exceeds its alpha.
            let [r, g, b, a] = px.0;
            Color32::from_rgba_premultiplied(r.min(a), g.min(a), b.min(a), a)
        })
        .collect();
    ColorImage::new([w as usize, h as usize], pixels)
}

/// The textures made so far, one per picture (the width they were cut to, and the texture).
type Made = HashMap<Art, (u32, TextureHandle)>;

/// The texture of `art` cut to `width_px`, made on first use.
fn texture(ctx: &egui::Context, art: Art, width_px: u32) -> TextureHandle {
    let key = egui::Id::NULL.with("altrune-brand-textures");
    let width_px = pixel_size(art, width_px)[0];
    let known = ctx.data(|d| d.get_temp::<Made>(key).unwrap_or_default());
    if let Some((px, tex)) = known.get(&art) {
        if *px == width_px {
            return tex.clone();
        }
    }
    let tex = ctx.load_texture(
        format!("altrune-{art:?}-{width_px}"),
        scaled(art, width_px),
        TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| {
        let mut made = d.get_temp::<Made>(key).unwrap_or_default();
        made.insert(art, (width_px, tex.clone()));
        d.insert_temp(key, made);
    });
    tex
}

/// The size in points of a picture meant to be `width_pt` wide: a whole number of device pixels
/// in each direction (never beyond what the original has), so it is shown one to one.
pub fn size_pt(ctx: &egui::Context, art: Art, width_pt: f32) -> egui::Vec2 {
    let ppp = ctx.pixels_per_point();
    let [w, h] = pixel_size(art, (width_pt * ppp).round().max(1.0) as u32);
    vec2(w as f32 / ppp, h as f32 / ppp)
}

/// Where a picture `width_pt` wide goes: its top-left corner `at`, moved onto the pixel grid, so
/// nothing is blurred by landing between pixels.
pub fn place(ctx: &egui::Context, art: Art, at: egui::Pos2, width_pt: f32) -> Rect {
    let ppp = ctx.pixels_per_point();
    let min = pos2((at.x * ppp).round() / ppp, (at.y * ppp).round() / ppp);
    Rect::from_min_size(min, size_pt(ctx, art, width_pt))
}

/// The same, with the picture centred on `center` instead of started at a corner.
pub fn place_centered(ctx: &egui::Context, art: Art, center: egui::Pos2, width_pt: f32) -> Rect {
    let half = size_pt(ctx, art, width_pt) / 2.0;
    place(ctx, art, center - half, width_pt)
}

/// Draws `art` in `rect` (from [`place`]), tinted by `tint` (white leaves its colours as they
/// are).
pub fn paint(painter: &egui::Painter, art: Art, rect: Rect, tint: Color32) {
    let ctx = painter.ctx();
    let px = (rect.width() * ctx.pixels_per_point()).round() as u32;
    let tex = texture(ctx, art, px);
    painter.image(
        tex.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        tint,
    );
}

/// The textures made so far as (picture, width in pixels, texture), for the tests.
#[cfg(test)]
pub fn textures_made(ctx: &egui::Context) -> Vec<(Art, u32, egui::TextureId)> {
    let key = egui::Id::NULL.with("altrune-brand-textures");
    let mut made: Vec<_> = ctx
        .data(|d| d.get_temp::<Made>(key).unwrap_or_default())
        .into_iter()
        .map(|(art, (px, tex))| (art, px, tex.id()))
        .collect();
    made.sort_by_key(|m| m.0.index());
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture is cut and uploaded once: asking again gives the same texture, and only a new
    /// width (another screen density) makes a new one, replacing the old.
    #[test]
    fn a_texture_is_made_once_per_size() {
        let ctx = egui::Context::default();
        let first = texture(&ctx, Art::Symbol, 44);
        for _ in 0..5 {
            assert_eq!(texture(&ctx, Art::Symbol, 44).id(), first.id());
        }
        assert_eq!(textures_made(&ctx).len(), 1);
        let other = texture(&ctx, Art::Symbol, 66);
        assert_ne!(other.id(), first.id());
        let made = textures_made(&ctx);
        assert_eq!(made.len(), 1, "the old width is dropped");
        assert_eq!(made[0].1, 66);
        // Another picture has its own.
        let _ = texture(&ctx, Art::LogoWord, 300);
        assert_eq!(textures_made(&ctx).len(), 2);
    }

    /// Every embedded picture, to check that the files are good.
    fn all() -> [Art; 3] {
        [Art::Symbol, Art::LogoSymbol, Art::LogoWord]
    }

    #[test]
    fn every_embedded_picture_decodes_with_its_transparency() {
        for art in all() {
            let [w, h] = native_size(art);
            assert!(w > 1 && h > 1, "{art:?} decodes ({w} x {h})");
            let img = decoded(art);
            assert!(
                img.pixels().any(|p| p.0[3] == 0),
                "{art:?} has clear pixels"
            );
            assert!(
                img.pixels().any(|p| p.0[3] == 255),
                "{art:?} has solid pixels"
            );
        }
    }

    /// The two layers of the logo share one canvas, so drawn in one rectangle they recompose it.
    #[test]
    fn the_logo_layers_share_their_canvas() {
        assert_eq!(native_size(Art::LogoSymbol), native_size(Art::LogoWord));
        for width in [100, 313, 512, 900] {
            assert_eq!(
                pixel_size(Art::LogoSymbol, width),
                pixel_size(Art::LogoWord, width)
            );
        }
    }

    #[test]
    fn the_wordmark_is_a_white_mask() {
        // Premultiplied white is its alpha in every channel: the app tints it with the text colour.
        let img = decoded(Art::LogoWord);
        assert!(img
            .pixels()
            .all(|p| p.0[0] == p.0[3] && p.0[1] == p.0[3] && p.0[2] == p.0[3]));
    }

    #[test]
    fn sizes_keep_the_proportions_and_never_grow_past_the_original() {
        let [w, h] = native_size(Art::Symbol);
        assert_eq!(pixel_size(Art::Symbol, w), [w, h]);
        assert_eq!(pixel_size(Art::Symbol, w * 3), [w, h]);
        let [sw, sh] = pixel_size(Art::Symbol, 44);
        assert_eq!(sw, 44);
        let wanted = 44.0 * h as f32 / w as f32;
        assert!((sh as f32 - wanted).abs() <= 0.5, "{sh} vs {wanted}");
        assert_eq!(pixel_size(Art::Symbol, 0)[0], 1);
    }

    /// Cutting a picture down keeps a premultiplied colour within its alpha (no glow at the
    /// edges) and keeps the transparency around the symbol.
    #[test]
    fn a_cut_down_picture_is_clean_at_the_edges() {
        for art in all() {
            for width in [22, 44, 66, 200, 400] {
                let img = scaled(art, width);
                assert_eq!(img.size[0] as u32, pixel_size(art, width)[0]);
                for c in &img.pixels {
                    assert!(
                        c.r() <= c.a() && c.g() <= c.a() && c.b() <= c.a(),
                        "{art:?}"
                    );
                }
                assert_eq!(
                    img.pixels[0].a(),
                    0,
                    "{art:?} corner stays clear at {width}"
                );
            }
        }
    }

    #[test]
    fn a_picture_lands_on_whole_device_pixels() {
        let ctx = egui::Context::default();
        for ppp in [1.0, 2.0, 3.0] {
            ctx.set_pixels_per_point(ppp);
            let r = place(&ctx, Art::Symbol, pos2(10.3, 7.7), 22.0);
            let on_grid = |v: f32| ((v * ppp).round() - v * ppp).abs() < 1e-3;
            assert!(
                on_grid(r.min.x) && on_grid(r.min.y) && on_grid(r.width()) && on_grid(r.height())
            );
            assert!((r.width() - 22.0).abs() <= 0.5 / ppp);
        }
    }
}
