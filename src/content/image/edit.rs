use std::sync::{Arc, OnceLock};
use std::time::Instant;

use crate::content::types::CachedImage;

use super::super::Rarity;
use anyhow::Ok;
use bytes::Bytes;
use skia_rs_canvas::{Canvas, Surface};
use skia_rs_codec::{Image, ImageEncoder, ImageInfo, JpegEncoder, decode_image};
use skia_rs_core::Color;
use skia_rs_paint::{Paint, Style};
use skia_rs_text::{Font, TextBlobBuilder, Typeface};

const IMPACT_TTF: &[u8] = include_bytes!("../../../impact.ttf");
const ARIAL_TTF: &[u8] = include_bytes!("../../../arial.ttf");

/// The Impact typeface is immutable and identical across every request, but
/// `Typeface::from_data` re-parses the font bytes (and re-runs font shaping
/// setup) every time it's called. Parse it once and hand out cheap `Arc`
/// clones from then on.
static IMPACT_TYPEFACE: OnceLock<Arc<Typeface>> = OnceLock::new();

fn impact_typeface() -> anyhow::Result<Arc<Typeface>> {
    if let Some(tf) = IMPACT_TYPEFACE.get() {
        return Ok(tf.clone());
    }

    log::debug!("Parsing Impact typeface (first use, will be cached)");
    let tf = Arc::new(
        Typeface::from_data(IMPACT_TTF.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Cannot initialize font"))?,
    );

    // If another thread raced us and initialized it first, use theirs and
    // drop ours, so there's always exactly one cached instance.
    Ok(IMPACT_TYPEFACE.get_or_init(|| tf).clone())
}

pub fn process_image(
    rarity: &dyn Rarity,
    image: Bytes,
    text: String,
) -> anyhow::Result<CachedImage> {
    let start = Instant::now();

    let image = decode_image(&image)?;
    let (w, h) = image.dimensions();
    let scaling = 800 - w;
    log::trace!("Scaling image {w}x{h} -> {}x{}", w + scaling, h + scaling);
    let image = image
        .make_scaled_with(
            w + scaling,
            h + scaling,
            skia_rs_codec::SamplingOptions::Linear,
        )
        .ok_or_else(|| anyhow::anyhow!("Cannot scale image"))?;

    let image = if let Some(filter) = rarity.filter() {
        log::trace!("Applying filter for rarity {}", rarity.name());
        image
            .make_with_filter(filter.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Cannot apply filter"))?
    } else {
        image
    };

    let (w, h) = image.dimensions();
    let mut surface = Surface::new_raster_n32_premul(w, h)
        .ok_or_else(|| anyhow::anyhow!("Cannot create surface"))?;
    let mut canvas = surface.canvas();
    canvas.draw_image(&image, 0.0, 0.0, None);

    let typeface = impact_typeface()?;
    canvas = bake_caption(canvas, &text, typeface)?;

    let baked = surface
        .make_image_snapshot()
        .ok_or_else(|| anyhow::anyhow!("Cannot create snapshot"))?;

    let encode_start = Instant::now();
    let mut out: Vec<u8> = Vec::new();
    JpegEncoder::default().encode(&baked, &mut out)?;
    log::trace!(
        "JPEG encode took {:?} ({} bytes)",
        encode_start.elapsed(),
        out.len()
    );

    log::debug!(
        "process_image total: {:?} (rarity={})",
        start.elapsed(),
        rarity.name()
    );

    Ok(CachedImage::from_bytes(out))
}

pub fn bake_caption<'a>(
    mut canvas: Canvas<'a>,
    text: &str,
    typeface: Arc<Typeface>,
) -> anyhow::Result<Canvas<'a>> {
    let w_f = canvas.width() as f32;
    let h_f = canvas.height() as f32;

    // --- layout constraints ---
    let side_margin = w_f * 0.05; // 5% margin each side
    let max_text_width = w_f - side_margin * 2.0;
    let max_block_height = h_f * 0.25; // don't eat more than 25% of the image
    let bottom_margin = h_f * 0.04;

    let (font_size, mut lines) = fit_text(
        text,
        &typeface,
        max_text_width,
        max_block_height,
        96.0,
        12.0,
    );
    log::debug!(
        "Caption fit: font_size={font_size}, lines={}, chars={}",
        lines.len(),
        text.len()
    );

    // --- draw ---
    let font = Font::new(typeface, font_size);
    let line_height = font_size * 1.15; // Impact is tight-leaded; tweak to taste
    let block_height = line_height * lines.len() as f32;
    let mut y = h_f - bottom_margin - block_height + font_size; // baseline of first line

    let mut fill = Paint::new();
    fill.set_color(Color::WHITE.into());
    fill.set_anti_alias(true);
    fill.set_style(Style::Fill);

    let mut stroke = Paint::new();
    stroke.set_color(Color::BLACK.into());
    stroke.set_anti_alias(true);
    stroke.set_style(Style::Stroke);
    stroke.set_stroke_width((font_size * 0.06).max(2.0));

    for line in lines.drain(..) {
        let line_width = font.measure_text(&line);
        let x = (w_f - line_width) / 2.0; // centered

        // outline first, fill on top — classic meme look
        canvas.draw_string(&line, x, y, &font, &stroke);
        canvas.draw_string(&line, x, y, &font, &fill);

        y += line_height;
    }

    Ok(canvas)
}

/// Try decreasing font sizes until the wrapped text fits both
/// `max_width` per line and `max_height` total. Returns (font_size, lines).
///
/// Rather than always linear-stepping from `start_size` down to a fit (which
/// can take many measure/wrap passes for long captions), we measure once at
/// `start_size`, estimate how much smaller the font needs to be from the
/// overflow ratio, jump straight there, and only linear-step a couple of
/// times afterward to correct for wrapping's non-linearity.
fn fit_text(
    text: &str,
    typeface: &Arc<Typeface>,
    max_width: f32,
    max_height: f32,
    start_size: f32,
    min_size: f32,
) -> (f32, Vec<String>) {
    // Measure once at start_size on the whole (unwrapped) text to estimate scale.
    let probe_font = Font::new(typeface.clone(), start_size);
    let probe_width = probe_font.measure_text(text).max(1.0);

    // Estimate a starting size assuming width scales ~linearly with font size,
    // then clamp into range. This skips most of the step-down iterations.
    let estimated = (start_size * (max_width / probe_width)).clamp(min_size, start_size);

    let mut size = estimated;
    loop {
        let font = Font::new(typeface.clone(), size);
        let lines = word_wrap(text, &font, max_width);
        let line_height = size * 1.15;
        let total_height = line_height * lines.len() as f32;

        if total_height <= max_height || size <= min_size {
            return (size, lines);
        }
        // Still too big after the estimate — smaller step now since we're close.
        size = (size - 4.0).max(min_size);
    }
}

/// Greedy word-wrap: pack words into lines while each line's
/// measured width stays under `max_width`.
fn word_wrap(text: &str, font: &Font, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };

        if font.measure_text(&candidate) <= max_width || current.is_empty() {
            // either it fits, or it's a single overlong word we can't split further
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
