use std::sync::Arc;

use chrono::{Local, Timelike};
use rand::{RngExt, random, rng};
use skia_rs_codec::{ImageInfo, decode_image};
use skia_rs_core::{AlphaType, Color, ColorType, Rect};
use skia_rs_paint::{ImageFilter, ImageFilterRef};

use super::{Rarity, SimpleRarity};
use crate::register_rarity;
use crate::text::{Pos, WordSpec};

// =========================================================================
//  STATIC ASSETS
// =========================================================================

const BLUE_CAT_STATIC_BYTES: &[u8] = include_bytes!("../../../placeholder.jpg");

// =========================================================================
//  PROCEDURAL IMAGE FILTERS
// =========================================================================

#[derive(Debug, Clone)]
struct StaticAssetFilter {
    bytes: &'static [u8],
}

impl ImageFilter for StaticAssetFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        if let Ok(decoded) = decode_image(self.bytes) {
            let info = ImageInfo::new(
                width as i32,
                height as i32,
                ColorType::Rgba8888,
                AlphaType::Premul,
            );
            decoded.read_pixels(&info, pixels, width * 4, 0, 0);
        }
    }
}

#[derive(Debug, Clone)]
struct HolographicFoilFilter;

impl ImageFilter for HolographicFoilFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let wf = width as f32;
        let hf = height as f32;
        let diag = (wf * wf + hf * hf).sqrt().max(1.0);

        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let r_base = pixels[idx] as f32;
                let g_base = pixels[idx + 1] as f32;
                let b_base = pixels[idx + 2] as f32;
                let lum = (0.299 * r_base + 0.587 * g_base + 0.114 * b_base) / 255.0;

                let d = (xf * 0.85 + yf * 0.65) / diag;
                let phase = d * 20.0;

                let r_spec = (phase.sin() * 0.5 + 0.5) * 255.0;
                let g_spec = ((phase + 2.094).sin() * 0.5 + 0.5) * 255.0;
                let b_spec = ((phase + 4.188).sin() * 0.5 + 0.5) * 255.0;

                let shimmer = ((phase * 2.2).sin().powi(4) * 0.65).max(0.0);
                let foil_strength = 0.45 + lum * 0.35;

                let r = (r_base * (1.0 - foil_strength)
                    + (r_spec + shimmer * 120.0) * foil_strength * lum)
                    .clamp(0.0, 255.0);
                let g = (g_base * (1.0 - foil_strength)
                    + (g_spec + shimmer * 120.0) * foil_strength * lum)
                    .clamp(0.0, 255.0);
                let b = (b_base * (1.0 - foil_strength)
                    + (b_spec + shimmer * 120.0) * foil_strength * lum)
                    .clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MatrixRainFilter;

impl ImageFilter for MatrixRainFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let scanline = if y % 3 == 0 { 0.84 } else { 1.0 };
            for x in 0..width {
                let idx = (y * width + x) * 4;
                let lum = (0.299 * pixels[idx] as f32
                    + 0.587 * pixels[idx + 1] as f32
                    + 0.114 * pixels[idx + 2] as f32)
                    / 255.0;

                let col_seed = ((x / 12) * 1103515245 + 12345) as f32;
                let stream_pos =
                    ((y as f32 * 0.075 + col_seed * 0.0001).sin() * 0.5 + 0.5).powf(3.0);

                let r = (lum * 30.0 + stream_pos * 40.0) * scanline;
                let g = (lum * 220.0 + stream_pos * 90.0 + 15.0) * scanline;
                let b = (lum * 60.0 + stream_pos * 50.0) * scanline;

                pixels[idx] = r.clamp(0.0, 255.0) as u8;
                pixels[idx + 1] = g.clamp(0.0, 255.0) as u8;
                pixels[idx + 2] = b.clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct CosmicNebulaFilter;

impl ImageFilter for CosmicNebulaFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;
                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;
                let lum = (0.299 * r_in + 0.587 * g_in + 0.114 * b_in) / 255.0;

                let neb_r = (lum.powf(0.8) * 210.0 + (1.0 - lum) * 45.0).clamp(0.0, 255.0);
                let neb_g = (lum.powf(1.4) * 150.0 + (1.0 - lum) * 10.0).clamp(0.0, 255.0);
                let neb_b = (lum.powf(0.7) * 255.0 + (1.0 - lum) * 80.0).clamp(0.0, 255.0);

                let hash = ((xf * 12.9898 + yf * 78.233).sin() * 43758.5453).fract();
                let star = if hash > 0.986 && lum < 0.65 {
                    (hash - 0.986) / 0.014 * 190.0
                } else {
                    0.0
                };

                let r = (r_in * 0.42 + neb_r * 0.58 + star).clamp(0.0, 255.0);
                let g = (g_in * 0.35 + neb_g * 0.65 + star * 0.9).clamp(0.0, 255.0);
                let b = (b_in * 0.40 + neb_b * 0.60 + star).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct DivineRadianceFilter;

impl ImageFilter for DivineRadianceFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let wf = width as f32;
        let hf = height as f32;

        for y in 0..height {
            let y_norm = y as f32 / hf.max(1.0);
            for x in 0..width {
                let x_norm = x as f32 / wf.max(1.0);
                let idx = (y * width + x) * 4;

                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;

                let beam = ((x_norm * 12.0 + y_norm * 3.5).sin() * 0.5 + 0.5).powi(3)
                    * (1.0 - y_norm * 0.55)
                    * 65.0;
                let glow = (1.0 - y_norm * 0.45) * 35.0;

                let r = (r_in * 1.25 + glow + beam + 22.0).clamp(0.0, 255.0);
                let g = (g_in * 1.20 + glow * 0.95 + beam * 0.9 + 18.0).clamp(0.0, 255.0);
                let b = (b_in * 1.05 + glow * 0.70 + beam * 0.6 + 10.0).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AbyssalDepthsFilter;

impl ImageFilter for AbyssalDepthsFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;

                let wave = ((xf * 0.026 + (yf * 0.03).sin() * 2.8).sin()
                    * (yf * 0.026 + (xf * 0.03).sin() * 2.8).cos())
                .abs();
                let caustic = wave * 40.0;

                let dist = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() / max_r;
                let vignette = (1.15 - dist * 0.65).clamp(0.2, 1.0);

                let r = (r_in * 0.28 * vignette + caustic * 0.3).clamp(0.0, 255.0);
                let g = (g_in * 1.05 * vignette + caustic * 0.8 + 12.0).clamp(0.0, 255.0);
                let b = (b_in * 1.45 * vignette + caustic + 25.0).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RetroVhsFilter {
    jitter: usize,
}

impl ImageFilter for RetroVhsFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let copy = pixels.to_vec();
        let disp = self.jitter.min(width / 8);

        for y in 0..height {
            let is_scanline = y % 2 == 0;
            let scan_mult = if is_scanline { 0.82 } else { 1.0 };
            let row_offset = y * width * 4;
            let tracking_shift = if (y / 10) % 19 == 0 {
                (y % 8) as i32 - 4
            } else {
                0
            };

            for x in 0..width {
                let idx = row_offset + x * 4;

                let r_x =
                    ((x as i32 - disp as i32 + tracking_shift).clamp(0, width as i32 - 1)) as usize;
                let b_x =
                    ((x as i32 + disp as i32 + tracking_shift).clamp(0, width as i32 - 1)) as usize;

                let r = copy[row_offset + r_x * 4] as f32 * scan_mult;
                let g = copy[idx + 1] as f32 * scan_mult;
                let b = copy[row_offset + b_x * 4 + 2] as f32 * scan_mult;

                let r_out = (r * 1.15 + 10.0).clamp(0.0, 255.0);
                let g_out = (g * 0.95 + 8.0).clamp(0.0, 255.0);
                let b_out = (b * 1.10 + 15.0).clamp(0.0, 255.0);

                pixels[idx] = r_out as u8;
                pixels[idx + 1] = g_out as u8;
                pixels[idx + 2] = b_out as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct GlacialCrystalFilter;

impl ImageFilter for GlacialCrystalFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;
                let lum = (0.299 * r_in + 0.587 * g_in + 0.114 * b_in) / 255.0;

                let facet = ((xf * 0.05 + yf * 0.05).sin() * (xf * 0.05 - yf * 0.05).cos())
                    .abs()
                    .powi(3)
                    * 55.0;

                let r = (r_in * 0.55 + facet * 0.4 + lum * 40.0).clamp(0.0, 255.0);
                let g = (g_in * 0.90 + facet * 0.8 + lum * 70.0 + 20.0).clamp(0.0, 255.0);
                let b = (b_in * 1.35 + facet + lum * 90.0 + 40.0).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RadioactiveHazardFilter;

impl ImageFilter for RadioactiveHazardFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let lum = (0.299 * pixels[idx] as f32
                    + 0.587 * pixels[idx + 1] as f32
                    + 0.114 * pixels[idx + 2] as f32)
                    / 255.0;

                let noise = (((xf * 91.34 + yf * 47.89).sin() * 43758.5453).fract() - 0.5) * 30.0;

                let r = (lum.powf(2.0) * 180.0 + noise * 0.5).clamp(0.0, 255.0);
                let g = (lum.powf(0.7) * 255.0 + 20.0 + noise).clamp(0.0, 255.0);
                let b = (lum.powf(3.0) * 80.0 + noise * 0.3).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct BloodMoonFilter;

impl ImageFilter for BloodMoonFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let lum = (0.299 * pixels[idx] as f32
                    + 0.587 * pixels[idx + 1] as f32
                    + 0.114 * pixels[idx + 2] as f32)
                    / 255.0;

                let dist = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() / max_r;
                let vignette = (1.2 - dist * 0.7).clamp(0.2, 1.0);

                let r = ((lum.powf(0.8) * 255.0 + 20.0) * vignette).clamp(0.0, 255.0);
                let g = ((lum.powf(2.2) * 55.0) * vignette).clamp(0.0, 255.0);
                let b = ((lum.powf(2.0) * 45.0 + (1.0 - lum) * 20.0) * vignette).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MidnightFilter;

impl ImageFilter for MidnightFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let lum = (0.299 * pixels[idx] as f32
                    + 0.587 * pixels[idx + 1] as f32
                    + 0.114 * pixels[idx + 2] as f32)
                    / 255.0;
                let dist = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() / max_r;
                let vignette = (1.1 - dist * 0.55).clamp(0.25, 1.0);

                let r = ((lum.powf(1.6) * 140.0 + 8.0) * vignette).clamp(0.0, 255.0);
                let g = ((lum.powf(1.2) * 190.0 + 15.0) * vignette).clamp(0.0, 255.0);
                let b = ((lum.powf(0.85) * 255.0 + 35.0) * vignette).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct InfernalFilter;

impl ImageFilter for InfernalFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;
                let lum = (0.299 * r_in + 0.587 * g_in + 0.114 * b_in) / 255.0;

                let heat = ((yf * 0.06 + (xf * 0.05).sin() * 2.0).sin() * 0.5 + 0.5) * 25.0;

                let r = (lum.powf(0.65) * 255.0 + heat + 15.0).clamp(0.0, 255.0);
                let g = (lum.powf(1.4) * 140.0 + heat * 0.5).clamp(0.0, 255.0);
                let b = (lum.powf(2.8) * 40.0).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct GoldenHourFilter;

impl ImageFilter for GoldenHourFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let wf = width as f32;
        let hf = height as f32;

        for y in 0..height {
            let y_norm = y as f32 / hf.max(1.0);
            for x in 0..width {
                let x_norm = x as f32 / wf.max(1.0);
                let idx = (y * width + x) * 4;

                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;

                let sun_dist = ((x_norm - 0.85).powi(2) + (y_norm - 0.15).powi(2)).sqrt();
                let flare = (1.0 - sun_dist.min(1.0)).powi(2) * 55.0;

                let r = (r_in * 1.35 + flare + 18.0).clamp(0.0, 255.0);
                let g = (g_in * 1.05 + flare * 0.8 + 8.0).clamp(0.0, 255.0);
                let b = (b_in * 0.65 + flare * 0.3).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct VoidSingularityFilter;

impl ImageFilter for VoidSingularityFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist = (dx * dx + dy * dy).sqrt() / max_r;

                let core_darken = (dist * 2.2).min(1.0);
                let edge_darken = (1.2 - dist).clamp(0.2, 1.0);
                let factor = core_darken * edge_darken;

                let r = (pixels[idx] as f32 * 0.7 * factor).clamp(0.0, 255.0);
                let g = (pixels[idx + 1] as f32 * 0.2 * factor).clamp(0.0, 255.0);
                let b = (pixels[idx + 2] as f32 * 1.3 * factor).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct GlitchAberrationFilter {
    displacement_px: usize,
    scanline_intensity: f32,
    red_crush: f32,
}

impl ImageFilter for GlitchAberrationFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let copy = pixels.to_vec();
        let disp = self.displacement_px.min(width / 4);

        for y in 0..height {
            let is_scanline = y % 4 == 0;
            let row_offset = y * width * 4;

            for x in 0..width {
                let idx = row_offset + x * 4;

                let r_x = if x >= disp { x - disp } else { 0 };
                let b_x = if x + disp < width {
                    x + disp
                } else {
                    width - 1
                };

                let r_idx = row_offset + r_x * 4;
                let b_idx = row_offset + b_x * 4;

                let mut r = (copy[r_idx] as f32 * self.red_crush).clamp(0.0, 255.0);
                let mut g = copy[idx + 1] as f32;
                let mut b = copy[b_idx + 2] as f32;

                if is_scanline {
                    r *= 1.0 - self.scanline_intensity;
                    g *= 1.0 - self.scanline_intensity;
                    b *= 1.0 - self.scanline_intensity;
                }

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ThermalVisionFilter;

impl ImageFilter for ThermalVisionFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], _width: usize, _height: usize) {
        for chunk in pixels.chunks_exact_mut(4) {
            let l = (0.299 * chunk[0] as f32 + 0.587 * chunk[1] as f32 + 0.114 * chunk[2] as f32)
                / 255.0;

            let (r, g, b) = if l < 0.25 {
                let t = l / 0.25;
                (0.0, t * 30.0, t * 180.0)
            } else if l < 0.50 {
                let t = (l - 0.25) / 0.25;
                (t * 200.0, 0.0, 180.0 + t * 40.0)
            } else if l < 0.75 {
                let t = (l - 0.50) / 0.25;
                (200.0 + t * 55.0, t * 200.0, 220.0 * (1.0 - t))
            } else {
                let t = (l - 0.75) / 0.25;
                (255.0, 200.0 + t * 55.0, t * 255.0)
            };

            chunk[0] = r.clamp(0.0, 255.0) as u8;
            chunk[1] = g.clamp(0.0, 255.0) as u8;
            chunk[2] = b.clamp(0.0, 255.0) as u8;
        }
    }
}

#[derive(Debug, Clone)]
struct NoirFilmFilter;

impl ImageFilter for NoirFilmFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let lum = (0.299 * pixels[idx] as f32
                    + 0.587 * pixels[idx + 1] as f32
                    + 0.114 * pixels[idx + 2] as f32)
                    / 255.0;

                let grain = (((xf * 67.12 + yf * 89.43).sin() * 43758.5453).fract() - 0.5) * 18.0;
                let dist = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() / max_r;
                let vignette = (1.15 - dist * 0.55).clamp(0.25, 1.0);

                let high_contrast = if lum < 0.5 {
                    2.0 * lum * lum
                } else {
                    1.0 - 2.0 * (1.0 - lum) * (1.0 - lum)
                };

                let val = (high_contrast * 255.0 * vignette + grain).clamp(0.0, 255.0) as u8;

                pixels[idx] = val;
                pixels[idx + 1] = val;
                pixels[idx + 2] = val;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AlienXenoFilter;

impl ImageFilter for AlienXenoFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                let r_inv = 255.0 - pixels[idx] as f32;
                let g_inv = 255.0 - pixels[idx + 1] as f32;
                let b_inv = 255.0 - pixels[idx + 2] as f32;

                let r = (r_inv * 0.35 + b_inv * 0.75).clamp(0.0, 255.0);
                let g = (g_inv * 1.35 + 10.0).clamp(0.0, 255.0);
                let b = (b_inv * 1.20 + r_inv * 0.45).clamp(0.0, 255.0);

                pixels[idx] = r as u8;
                pixels[idx + 1] = g as u8;
                pixels[idx + 2] = b as u8;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct GrungeSepiaFilter;

impl ImageFilter for GrungeSepiaFilter {
    fn filter_bounds(&self, src: &Rect) -> Rect {
        *src
    }

    fn apply(&self, pixels: &mut [u8], width: usize, height: usize) {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..height {
            let yf = y as f32;
            for x in 0..width {
                let xf = x as f32;
                let idx = (y * width + x) * 4;

                let r_in = pixels[idx] as f32;
                let g_in = pixels[idx + 1] as f32;
                let b_in = pixels[idx + 2] as f32;

                let dist = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() / max_r;
                let vignette = (1.1 - dist * 0.6).clamp(0.2, 1.0);
                let grime = (((xf * 11.3 + yf * 17.7).sin() * 23421.43).fract() - 0.5) * 22.0;

                let r = (r_in * 0.393 + g_in * 0.769 + b_in * 0.189 + grime) * vignette;
                let g = (r_in * 0.349 + g_in * 0.686 + b_in * 0.168 + grime * 0.8) * vignette;
                let b = (r_in * 0.272 + g_in * 0.534 + b_in * 0.131 + grime * 0.5) * vignette;

                pixels[idx] = r.clamp(0.0, 255.0) as u8;
                pixels[idx + 1] = g.clamp(0.0, 255.0) as u8;
                pixels[idx + 2] = b.clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn build_themed_template(
    adjs: &'static str,
    nouns: &'static str,
    verbs: &'static str,
    sentence_chance: f32,
) -> Vec<WordSpec> {
    let mut rng = rng();
    if sentence_chance > 0.0 && rng.random_bool(sentence_chance as f64) {
        let variant = rng.random_range(0..4);
        match variant {
            0 => vec![
                WordSpec::new()
                    .poses(&[Pos::Adj])
                    .similar_to_text(adjs)
                    .top_k(32),
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
                WordSpec::new()
                    .poses(&[Pos::Verb])
                    .similar_to_text(verbs)
                    .top_k(32),
                WordSpec::new().poses(&[Pos::Noun]).top_k(48),
            ],
            1 => vec![
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
                WordSpec::new()
                    .poses(&[Pos::Verb])
                    .similar_to_text(verbs)
                    .top_k(32),
                WordSpec::fixed("в"),
                WordSpec::new()
                    .poses(&[Pos::Adj])
                    .similar_to_text(adjs)
                    .top_k(32),
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
            ],
            2 => vec![
                WordSpec::new()
                    .poses(&[Pos::Adj])
                    .similar_to_text(adjs)
                    .top_k(32),
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
                WordSpec::new()
                    .poses(&[Pos::Verb])
                    .similar_to_text(verbs)
                    .top_k(32),
                WordSpec::fixed("на"),
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
            ],
            _ => vec![
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
                WordSpec::new()
                    .poses(&[Pos::Verb])
                    .similar_to_text(verbs)
                    .top_k(32),
                WordSpec::new()
                    .poses(&[Pos::Adj])
                    .similar_to_text(adjs)
                    .top_k(32),
                WordSpec::new()
                    .poses(&[Pos::Noun])
                    .similar_to_text(nouns)
                    .top_k(48),
            ],
        }
    } else {
        vec![
            WordSpec::new()
                .poses(&[Pos::Adj])
                .similar_to_text(adjs)
                .top_k(32)
                .min_similarity(0.25),
            WordSpec::new()
                .poses(&[Pos::Noun])
                .similar_to_text(nouns)
                .top_k(48),
        ]
    }
}

// =========================================================================
//  SECRET & TIME-CONDITIONED RARITIES
// =========================================================================

#[derive(Debug)]
pub struct CursedRarity;

impl Rarity for CursedRarity {
    fn id(&self) -> String {
        "cursed".to_string()
    }
    fn name(&self) -> String {
        "ПРОКЛЯТЫЙ".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "occult"
    }
    fn tier(&self) -> f32 {
        0.50
    }
    fn is_secret(&self) -> bool {
        true
    }
    fn weight(&self) -> f32 {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();

        if hour == 3 && minute == 33 {
            100_000.0
        } else if hour == 3 {
            400.0
        } else {
            0.0
        }
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 0, 0)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 40, 40)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "проклятый кровавый мертвый кошмарный жуткий потусторонний гибельный",
            "кровь кошмар гибель смерть пепел проклятие ужас бездна",
            "преследовать проклинать пугать губить разрушать воскресать",
            0.20,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(GlitchAberrationFilter {
            displacement_px: 12,
            scanline_intensity: 0.6,
            red_crush: 1.8,
        }))
    }
}
register_rarity!(CursedRarity);

#[derive(Debug)]
pub struct BlueCatRarity;

impl Rarity for BlueCatRarity {
    fn id(&self) -> String {
        "blue_cat".to_string()
    }
    fn name(&self) -> String {
        "СИНИЙ КОТ".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        1.0
    }
    fn is_secret(&self) -> bool {
        true
    }
    fn weight(&self) -> f32 {
        0.1
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 65, 105, 225)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 0, 255, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        vec![WordSpec::fixed("синий"), WordSpec::fixed("кот")]
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(StaticAssetFilter {
            bytes: BLUE_CAT_STATIC_BYTES,
        }))
    }
}
register_rarity!(BlueCatRarity);

#[derive(Debug)]
pub struct GoldenHourRarity;

impl Rarity for GoldenHourRarity {
    fn id(&self) -> String {
        "golden_hour".to_string()
    }
    fn name(&self) -> String {
        "Золотой Час".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "celestial"
    }
    fn tier(&self) -> f32 {
        0.70
    }
    fn is_secret(&self) -> bool {
        true
    }
    fn weight(&self) -> f32 {
        let hour = Local::now().hour();
        if (6..=7).contains(&hour) || (18..=19).contains(&hour) {
            80.0
        } else {
            0.0
        }
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 191, 0)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 235, 150)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "золотой рассветный закатный сияющий янтарный медовый теплый",
            "рассвет закат солнце луч сияние горизонт полдень свет",
            "греть светить озарять сиять согревать ласкать",
            0.15,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(GoldenHourFilter))
    }
}
register_rarity!(GoldenHourRarity);

#[derive(Debug)]
pub struct MidnightRarity;

impl Rarity for MidnightRarity {
    fn id(&self) -> String {
        "midnight".to_string()
    }
    fn name(&self) -> String {
        "Полуночный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "celestial"
    }
    fn tier(&self) -> f32 {
        0.65
    }
    fn is_secret(&self) -> bool {
        true
    }
    fn weight(&self) -> f32 {
        let hour = Local::now().hour();
        if (0..=2).contains(&hour) { 70.0 } else { 0.0 }
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 100, 149, 237)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 180, 210, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "полуночный ночной лунный сумрачный сонный таинственный серебряный",
            "полночь луна сон тень ночь тишина туман шепот звезда",
            "бродить спать шептать скрываться наблюдать мерцать",
            0.18,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(MidnightFilter))
    }
}
register_rarity!(MidnightRarity);

#[derive(Debug)]
pub struct VoidRarity;

impl Rarity for VoidRarity {
    fn id(&self) -> String {
        "void".to_string()
    }
    fn name(&self) -> String {
        "ПУСТОТА".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "occult"
    }
    fn tier(&self) -> f32 {
        0.95
    }
    fn is_secret(&self) -> bool {
        true
    }
    fn weight(&self) -> f32 {
        1.2
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 138, 43, 226)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 200, 120, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "бездонный вакуумный сингулярный бесконечный темный гравитационный",
            "пустота бездна горизонт вакуум гравитация сингулярность мрак",
            "поглощать исчезать притягивать искажать молчать растворяться",
            0.20,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(VoidSingularityFilter))
    }
}
register_rarity!(VoidRarity);

// =========================================================================
//  THEMATIC VISUAL RARITIES
// =========================================================================

#[derive(Debug)]
pub struct HolographicRarity;

impl Rarity for HolographicRarity {
    fn id(&self) -> String {
        "holographic".to_string()
    }
    fn name(&self) -> String {
        "Голографический".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "cybernetic"
    }
    fn tier(&self) -> f32 {
        0.70
    }
    fn weight(&self) -> f32 {
        12.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 105, 180)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 0, 255, 240)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "радужный переливающийся призматический сияющий кристальный хромированный спектральный",
            "призма голограмма спектр кристалл сияние иллюзия отражение свет",
            "сиять переливаться отражать сверкать мерцать ослеплять",
            0.15,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(HolographicFoilFilter))
    }
}
register_rarity!(HolographicRarity);

#[derive(Debug)]
pub struct MatrixRarity;

impl Rarity for MatrixRarity {
    fn id(&self) -> String {
        "matrix".to_string()
    }
    fn name(&self) -> String {
        "Матричный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "cybernetic"
    }
    fn tier(&self) -> f32 {
        0.55
    }
    fn weight(&self) -> f32 {
        18.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 0, 255, 100)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 0, 255, 100)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "цифровой матричный программный код кибернетический поток терминал алгоритм сеть",
            "хакер система код матрица интерфейс протокол сеть процессор алгоритм",
            "взламывать вычислять кодировать передавать генерировать",
            0.16,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(MatrixRainFilter))
    }
}
register_rarity!(MatrixRarity);

#[derive(Debug)]
pub struct DivineRarity;

impl Rarity for DivineRarity {
    fn id(&self) -> String {
        "divine".to_string()
    }
    fn name(&self) -> String {
        "Божественный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "celestial"
    }
    fn tier(&self) -> f32 {
        0.95
    }
    fn weight(&self) -> f32 {
        3.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 245, 170)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 215, 0)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "божественный святой ангельский небесный благословенный сияющий вечный чистый",
            "ангел свет рай вечность создатель храм благодать крыло небеса",
            "благословлять сиять парить восходить защищать озарять",
            0.18,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(DivineRadianceFilter))
    }
}
register_rarity!(DivineRarity);

#[derive(Debug)]
pub struct CosmicRarity;

impl Rarity for CosmicRarity {
    fn id(&self) -> String {
        "cosmic".to_string()
    }
    fn name(&self) -> String {
        "Космический".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "celestial"
    }
    fn tier(&self) -> f32 {
        0.85
    }
    fn weight(&self) -> f32 {
        6.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 147, 112, 219)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 170, 140, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "космический галактический звездный астральный межзвездный туманный орбитальный",
            "космос звезда галактика туманность орбита комета вселенная астероид",
            "летать сиять парить вращаться сиять путешествовать",
            0.18,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(CosmicNebulaFilter))
    }
}
register_rarity!(CosmicRarity);

#[derive(Debug)]
pub struct InfernalRarity;

impl Rarity for InfernalRarity {
    fn id(&self) -> String {
        "infernal".to_string()
    }
    fn name(&self) -> String {
        "Адский".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "occult"
    }
    fn tier(&self) -> f32 {
        0.60
    }
    fn weight(&self) -> f32 {
        15.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 69, 0)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 140, 0)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "адский огненный пылающий демонический лавовый вулканический инфернальный яростный",
            "пламя огонь демон вулкан пепел ад бездна искра ярость",
            "пылать гореть сжигать пожирать бушевать воспламенять",
            0.15,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(InfernalFilter))
    }
}
register_rarity!(InfernalRarity);

#[derive(Debug)]
pub struct AbyssalRarity;

impl Rarity for AbyssalRarity {
    fn id(&self) -> String {
        "abyssal".to_string()
    }
    fn name(&self) -> String {
        "Глубоководный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "hazard"
    }
    fn tier(&self) -> f32 {
        0.30
    }
    fn weight(&self) -> f32 {
        28.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 0, 220, 220)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 80, 240, 240)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "глубоководный океанический подводный морской бездонный коралловый таинственный",
            "океан глубина бездна риф коралл волна жемчуг шторм течение",
            "плыть тонуть погружаться скрываться блуждать мерцать",
            0.12,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(AbyssalDepthsFilter))
    }
}
register_rarity!(AbyssalRarity);

#[derive(Debug)]
pub struct RetroVhsRarity;

impl Rarity for RetroVhsRarity {
    fn id(&self) -> String {
        "retro_vhs".to_string()
    }
    fn name(&self) -> String {
        "Ретро VHS".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "cybernetic"
    }
    fn tier(&self) -> f32 {
        0.10
    }
    fn weight(&self) -> f32 {
        35.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 175, 40)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 210, 80)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "ретро кассетный неоновый винтажный аналоговый ламповый ностальгический пленочный",
            "кассета видео пленка помеха экран звук запись аркада ритм",
            "играть звучать записывать мигать шуметь транслировать",
            0.10,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(RetroVhsFilter { jitter: 7 }))
    }
}
register_rarity!(RetroVhsRarity);

#[derive(Debug)]
pub struct GlacialRarity;

impl Rarity for GlacialRarity {
    fn id(&self) -> String {
        "glacial".to_string()
    }
    fn name(&self) -> String {
        "Кристальный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "hazard"
    }
    fn tier(&self) -> f32 {
        0.45
    }
    fn weight(&self) -> f32 {
        20.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 130, 220, 255)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 190, 240, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "ледяной морозный хрустальный снежный полярный алмазный северный чистый",
            "лед мороз снег кристалл иней стужа метель айсберг алмаз",
            "замерзать блестеть охлаждать сверкать сковывать сиять",
            0.14,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(GlacialCrystalFilter))
    }
}
register_rarity!(GlacialRarity);

#[derive(Debug)]
pub struct ToxicRarity;

impl Rarity for ToxicRarity {
    fn id(&self) -> String {
        "toxic".to_string()
    }
    fn name(&self) -> String {
        "Токсичный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "hazard"
    }
    fn tier(&self) -> f32 {
        0.45
    }
    fn weight(&self) -> f32 {
        25.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 57, 255, 20)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 180, 255, 50)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "радиоактивный ядовитый токсичный мутантный изотопный ядерный кислотный опасный",
            "радиация изотоп яд мутант кислота реактор бункер луч энергия",
            "светиться мутировать заражать распадаться пульсировать гудеть",
            0.10,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(RadioactiveHazardFilter))
    }
}
register_rarity!(ToxicRarity);

#[derive(Debug)]
pub struct BloodMoonRarity;

impl Rarity for BloodMoonRarity {
    fn id(&self) -> String {
        "blood_moon".to_string()
    }
    fn name(&self) -> String {
        "Кровавая Луна".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "occult"
    }
    fn tier(&self) -> f32 {
        0.75
    }
    fn weight(&self) -> f32 {
        10.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 220, 20, 60)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 60, 80)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "кровавый багровый затменный лунный зловещий полуночный сумрачный алый",
            "луна затмение кровь ночь мрак тень полнолуние ужас культ",
            "восходить затмевать пугать сиять властвовать наступать",
            0.18,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(BloodMoonFilter))
    }
}
register_rarity!(BloodMoonRarity);

#[derive(Debug)]
pub struct ThermalRarity;

impl Rarity for ThermalRarity {
    fn id(&self) -> String {
        "thermal".to_string()
    }
    fn name(&self) -> String {
        "Тепловизор".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "cybernetic"
    }
    fn tier(&self) -> f32 {
        0.35
    }
    fn weight(&self) -> f32 {
        25.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 57, 255, 20)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 140, 0)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "инфракрасный тепловой оптический сканирующий сенсорный лазерный",
            "сканер сенсор тепло луч сигнал фокус спектр",
            "сканировать отслеживать улавливать измерять фиксировать",
            0.0,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(ThermalVisionFilter))
    }
}
register_rarity!(ThermalRarity);

#[derive(Debug)]
pub struct AlienRarity;

impl Rarity for AlienRarity {
    fn id(&self) -> String {
        "alien".to_string()
    }
    fn name(&self) -> String {
        "Инопланетный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "cybernetic"
    }
    fn tier(&self) -> f32 {
        0.40
    }
    fn weight(&self) -> f32 {
        25.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 0, 255, 255)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 0, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "инопланетный внеземной марсианский ксеноморфный аномальный космический",
            "пришелец тарелка аномалия космос сигнал ксеноморф корабль",
            "вторгаться наблюдать похищать передавать транслировать",
            0.0,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(AlienXenoFilter))
    }
}
register_rarity!(AlienRarity);

#[derive(Debug)]
pub struct MonochromeRarity;

impl Rarity for MonochromeRarity {
    fn id(&self) -> String {
        "monochrome".to_string()
    }
    fn name(&self) -> String {
        "Монохромный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        -0.05
    }
    fn weight(&self) -> f32 {
        40.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 211, 211, 211)
    }
    fn text_color(&self) -> Color {
        Color::WHITE
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "мрачный детективный серый печальный дождливый винтажный ночной",
            "туман тень дождь вечер силуэт фонарь улица тайна",
            "бродить скрывать молчать ждать наблюдать исчезать",
            0.0,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(NoirFilmFilter))
    }
}
register_rarity!(MonochromeRarity);

#[derive(Debug)]
pub struct TrashRarity;

impl Rarity for TrashRarity {
    fn id(&self) -> String {
        "trash".to_string()
    }
    fn name(&self) -> String {
        "Мусорный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "hazard"
    }
    fn tier(&self) -> f32 {
        -1.0
    }
    fn weight(&self) -> f32 {
        100.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 139, 69, 19)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 175, 115, 60)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "грязный вонючий помойный тухлый ржавый липкий мятый",
            "мусор свалка хлам гниль отброс пакет банка",
            "гнить валяться вонять пачкать скрипеть ржаветь",
            0.0,
        )
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        Some(Arc::new(GrungeSepiaFilter))
    }
}
register_rarity!(TrashRarity);

// =========================================================================
//  STANDARD ROSTER
// =========================================================================

#[derive(Debug)]
pub struct CommonRarity;

impl Rarity for CommonRarity {
    fn id(&self) -> String {
        "common".to_string()
    }
    fn name(&self) -> String {
        "Обычный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        -0.80
    }
    fn weight(&self) -> f32 {
        220.0
    }
    fn rarity_color(&self) -> Color {
        Color::WHITE
    }
    fn text_color(&self) -> Color {
        Color::WHITE
    }
    fn template(&self) -> Vec<WordSpec> {
        vec![WordSpec::new()]
    }
}
register_rarity!(CommonRarity);

#[derive(Debug)]
pub struct UncommonRarity;

impl Rarity for UncommonRarity {
    fn id(&self) -> String {
        "uncommon".to_string()
    }
    fn name(&self) -> String {
        "Необычный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        -0.50
    }
    fn weight(&self) -> f32 {
        180.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 50, 205, 50)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 50, 205, 50)
    }
    fn template(&self) -> Vec<WordSpec> {
        vec![
            WordSpec::new()
                .similar_to_text("лесной крепкий зеленый шустрый умный веселый дикий храбрый")
                .top_k(48),
        ]
    }
}
register_rarity!(UncommonRarity);

#[derive(Debug)]
pub struct RareRarity;

impl Rarity for RareRarity {
    fn id(&self) -> String {
        "rare".to_string()
    }
    fn name(&self) -> String {
        "Редкий".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        -0.20
    }
    fn weight(&self) -> f32 {
        90.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 0, 140, 255)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 0, 140, 255)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "стальной острый быстрый горный северный зоркий ловкий редкий",
            "скала вершина сталь сокол мороз клинок буря ущелье",
            "рассекать парить преодолевать выслеживать взлетать",
            0.05,
        )
    }
}
register_rarity!(RareRarity);

#[derive(Debug)]
pub struct EpicRarity;

impl Rarity for EpicRarity {
    fn id(&self) -> String {
        "epic".to_string()
    }
    fn name(&self) -> String {
        "Эпический".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        0.35
    }
    fn weight(&self) -> f32 {
        28.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 163, 53, 238)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 163, 53, 238)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "магический эфирный королевский древний кристальный мистический рунный",
            "руна магия замок артефакт тайна кристалл корона заклинание",
            "колдовать созидать охранять пробуждать повелевать",
            0.15,
        )
    }
}
register_rarity!(EpicRarity);

#[derive(Debug)]
pub struct LegendaryRarity;

impl Rarity for LegendaryRarity {
    fn id(&self) -> String {
        "legendary".to_string()
    }
    fn name(&self) -> String {
        "Легендарный".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "neutral"
    }
    fn tier(&self) -> f32 {
        0.80
    }
    fn weight(&self) -> f32 {
        8.0
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 215, 0)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 215, 0)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "великий вечный золотой владыка абсолютный бесконечный верховный",
            "империя трон владыка вечность солнце корона победа слава",
            "править побеждать созидать освещать царствовать",
            0.20,
        )
    }
}
register_rarity!(LegendaryRarity);

#[derive(Debug)]
pub struct MythicalRarity;

impl Rarity for MythicalRarity {
    fn id(&self) -> String {
        "mythical".to_string()
    }
    fn name(&self) -> String {
        "Мифический".to_string()
    }
    fn domain_id(&self) -> &'static str {
        "occult"
    }
    fn tier(&self) -> f32 {
        0.98
    }
    fn weight(&self) -> f32 {
        1.5
    }
    fn rarity_color(&self) -> Color {
        Color::from_argb(255, 255, 20, 147)
    }
    fn text_color(&self) -> Color {
        Color::from_argb(255, 255, 20, 147)
    }
    fn template(&self) -> Vec<WordSpec> {
        build_themed_template(
            "потусторонний призрачный искаженный теневой хаотичный первородный",
            "миф бездна измерение портал сущность первородность хаос",
            "искажать трансформировать перемещать рождать разрушать",
            0.25,
        )
    }
}
register_rarity!(MythicalRarity);
