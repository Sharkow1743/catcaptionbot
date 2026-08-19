use std::sync::{Arc, OnceLock};
use std::time::Instant;

use super::super::Rarity;
use crate::content::rarity::RarityManager;
use crate::content::types::CachedImage;

use bytes::Bytes;
use skia_rs_canvas::{Canvas, Surface};
use skia_rs_codec::{Image, ImageEncoder, JpegEncoder, SamplingOptions, decode_image};
use skia_rs_core::{Color, Rect};
use skia_rs_paint::{Paint, Style};
use skia_rs_text::{Font, Typeface};

// =========================================================================
//  TWEAKABLE LAYOUT CONSTANTS (All base values are in pixels at 800px width)
// =========================================================================

pub const IMAGE_WIDTH: f32 = 800.0;
pub const SQUARE_FACTOR: f32 = 0.5;

// --- Rarity Badge Settings ---
pub const RARITY_FONT_SIZE: f32 = 30.0;
pub const RARITY_PAD_X: f32 = 12.0;
pub const RARITY_BADGE_HEIGHT: f32 = 42.0;
pub const RARITY_CORNER_RADIUS: f32 = 8.0;
pub const RARITY_BOTTOM_MARGIN: f32 = 5.0;
pub const RARITY_TEXT_Y_OFFSET: f32 = 30.0; // Distance from badge top to text baseline

// --- Caption Text Settings ---
pub const CAPTION_MAX_WIDTH: f32 = 720.0; // Horizontal boundary for caption text
pub const CAPTION_MAX_BLOCK_HEIGHT: f32 = 175.0; // Max vertical height caption can take
pub const CAPTION_MAX_FONT_SIZE: f32 = 96.0; // Starting font size for short text
pub const CAPTION_MIN_FONT_SIZE: f32 = 40.0; // Minimum size before it stops shrinking
pub const CAPTION_LINE_SPACING: f32 = -5.0; // Tight default gap between lines
pub const CAPTION_STROKE_WIDTH: f32 = 10.0; // Black outline thickness
pub const CAPTION_GAP_ABOVE_BADGE: f32 = 4.0; // Tight gap above rarity badge

// Extra spacing added between two lines ONLY if a descender on top (Д, Ц) or top diacritic on bottom (Й, Ё) collides
pub const LINE_COLLISION_EXTRA_SPACING: f32 = 10.0;

// Extra vertical shift applied ONLY when a descender (Д, Ц) on the bottom line collides with the badge
pub const DESCENDER_AVOID_SHIFT: f32 = 14.0;

// =========================================================================

const IMPACT_TTF: &[u8] = include_bytes!("../../../impact.ttf");
static IMPACT_TYPEFACE: OnceLock<Arc<Typeface>> = OnceLock::new();

const ARIAL_TTF: &[u8] = include_bytes!("../../../arial.ttf");
static ARIAL_TYPEFACE: OnceLock<Arc<Typeface>> = OnceLock::new();

fn impact_typeface() -> anyhow::Result<Arc<Typeface>> {
    if let Some(tf) = IMPACT_TYPEFACE.get() {
        return Ok(tf.clone());
    }

    let tf = Arc::new(
        Typeface::from_data(IMPACT_TTF.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Cannot initialize Impact font"))?,
    );

    Ok(IMPACT_TYPEFACE.get_or_init(|| tf).clone())
}

fn arial_typeface() -> anyhow::Result<Arc<Typeface>> {
    if let Some(tf) = ARIAL_TYPEFACE.get() {
        return Ok(tf.clone());
    }

    let tf = Arc::new(
        Typeface::from_data(ARIAL_TTF.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Cannot initialize Arial font"))?,
    );

    Ok(ARIAL_TYPEFACE.get_or_init(|| tf).clone())
}

pub struct BadgeLayout {
    pub top_from_bottom: f32,
    pub left: f32,
    pub right: f32,
}

struct CharSpan {
    ch: char,
    left: f32,
    right: f32,
}

fn is_descender(c: char) -> bool {
    matches!(
        c,
        'Д' | 'д' | 'Ц' | 'ц' | 'Щ' | 'щ' | 'р' | 'у' | 'ф' | 'g' | 'j' | 'p' | 'q' | 'y' | 'Q'
    )
}

fn is_top_diacritic(c: char) -> bool {
    matches!(
        c,
        'Й' | 'й'
            | 'Ё'
            | 'ё'
            | 'Ї'
            | 'ї'
            | 'Ў'
            | 'ў'
            | 'Â'
            | 'Ê'
            | 'Î'
            | 'Ô'
            | 'Û'
            | 'Ä'
            | 'Ë'
            | 'Ï'
            | 'Ö'
            | 'Ü'
            | 'Á'
            | 'É'
            | 'Í'
            | 'Ó'
            | 'Ú'
            | 'Å'
    )
}

fn get_char_spans(line: &str, font: &Font, line_x: f32) -> Vec<CharSpan> {
    let mut spans = Vec::with_capacity(line.chars().count());
    let mut prefix = String::with_capacity(line.len());
    let mut prev_w = 0.0;

    for ch in line.chars() {
        prefix.push(ch);
        let curr_w = font.measure_text(&prefix);
        if !ch.is_whitespace() {
            spans.push(CharSpan {
                ch,
                left: line_x + prev_w,
                right: line_x + curr_w,
            });
        }
        prev_w = curr_w;
    }
    spans
}

/// Checks if any descender on top_line hangs into bottom_line OR if any top diacritic on bottom_line reaches into top_line
fn lines_collide(
    top_line: &str,
    top_x: f32,
    bottom_line: &str,
    bottom_x: f32,
    font: &Font,
) -> bool {
    let top_spans = get_char_spans(top_line, font, top_x);
    let bottom_spans = get_char_spans(bottom_line, font, bottom_x);

    for top in &top_spans {
        for bottom in &bottom_spans {
            // Horizontal overlap
            if top.right > bottom.left && top.left < bottom.right {
                if is_descender(top.ch) || is_top_diacritic(bottom.ch) {
                    return true;
                }
            }
        }
    }
    false
}

/// Checks if any descender in the bottom line horizontally overlaps the badge
fn descender_collides_with_badge(
    last_line: &str,
    last_x: f32,
    font: &Font,
    badge_left: f32,
    badge_right: f32,
) -> bool {
    let spans = get_char_spans(last_line, font, last_x);
    spans
        .iter()
        .any(|s| is_descender(s.ch) && s.right > badge_left && s.left < badge_right)
}

pub fn preprocess_image(image: &Bytes) -> anyhow::Result<Image> {
    let image = decode_image(image)?;
    let (w, h) = image.dimensions();

    let proportional_h = h as f32 * (IMAGE_WIDTH / w as f32);

    let target_h = (proportional_h + (IMAGE_WIDTH - proportional_h) * SQUARE_FACTOR).round() as i32;
    let target_w = IMAGE_WIDTH as i32;

    image
        .make_scaled_with(target_w, target_h, SamplingOptions::Linear)
        .ok_or_else(|| anyhow::anyhow!("Cannot scale image"))
}

pub fn process_image(
    rarity: &dyn Rarity,
    image: Image,
    text: String,
) -> anyhow::Result<CachedImage> {
    let start = Instant::now();
    let (w, h) = image.dimensions();

    let mut surface = Surface::new_raster_n32_premul(w, h)
        .ok_or_else(|| anyhow::anyhow!("Cannot create surface"))?;
    let mut canvas = surface.canvas();

    if let Some(filter) = rarity.filter() {
        let filtered = image
            .make_with_filter(filter.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Cannot apply filter"))?;
        canvas.draw_image(&filtered, 0.0, 0.0, None);
    } else {
        canvas.draw_image(&image, 0.0, 0.0, None);
    }

    // Scale up for portrait images, bounded by MAX_ASPECT_SCALE
    let scale = h as f32 / IMAGE_WIDTH;

    // 1. Draw the bottom rarity banner
    let arial_tf = arial_typeface()?;
    let badge_layout = bake_rarity_bar(&mut canvas, arial_tf, rarity, scale)?;

    // 2. Draw the main Impact caption
    let impact_tf = impact_typeface()?;
    let upper_text = text.to_uppercase();
    bake_caption(
        &mut canvas,
        &upper_text,
        impact_tf,
        rarity,
        &badge_layout,
        scale,
    )?;

    let baked = surface
        .make_image_snapshot()
        .ok_or_else(|| anyhow::anyhow!("Cannot create snapshot"))?;

    let encode_start = Instant::now();
    let mut out: Vec<u8> = Vec::with_capacity((w * h) as usize / 4);
    JpegEncoder::default().encode(&baked, &mut out)?;

    log::info!(
        "Processed image in {:?}\nEncoding: {:?}",
        start.elapsed(),
        encode_start.elapsed()
    );

    Ok(CachedImage::from_bytes(out))
}

pub fn bake_rarity_bar(
    canvas: &mut Canvas,
    typeface: Arc<Typeface>,
    rarity: &dyn Rarity,
    scale: f32,
) -> anyhow::Result<BadgeLayout> {
    let h_f = canvas.height() as f32;

    let percentage = RarityManager::global().rarity_percentage(rarity);
    let label = format!("{} • {:.2}%", rarity.name(), percentage);

    let font_size = RARITY_FONT_SIZE * scale;
    let pad_x = RARITY_PAD_X * scale;
    let badge_h = RARITY_BADGE_HEIGHT * scale;
    let corner_radius = RARITY_CORNER_RADIUS * scale;
    let bottom_margin = RARITY_BOTTOM_MARGIN * scale;

    let mut font = Font::new(typeface, font_size);
    let mut text_width = font.measure_text(&label);

    // Ensure badge never overflows the canvas width
    let max_badge_w = IMAGE_WIDTH - 24.0;
    if text_width + pad_x * 2.0 > max_badge_w {
        let fit_font_size = font_size * (max_badge_w - pad_x * 2.0) / text_width;
        font.set_size(fit_font_size.max(12.0));
        text_width = font.measure_text(&label);
    }

    let badge_w = (text_width + pad_x * 2.0).min(max_badge_w);
    let badge_x = (IMAGE_WIDTH - badge_w) / 2.0;
    let badge_y = h_f - bottom_margin - badge_h;

    // Black background box
    let bg_rect = Rect::from_xywh(badge_x, badge_y, badge_w, badge_h);
    let mut bg_paint = Paint::new();
    bg_paint.set_color(Color::BLACK.into());
    bg_paint.set_anti_alias(true);
    bg_paint.set_style(Style::Fill);

    canvas.draw_round_rect(&bg_rect, corner_radius, corner_radius, &bg_paint);

    // Text with rarity color
    let mut text_paint = Paint::new();
    text_paint.set_color(rarity.rarity_color().into());
    text_paint.set_anti_alias(true);
    text_paint.set_style(Style::Fill);

    let text_x = badge_x + pad_x;
    let text_y = badge_y + (RARITY_TEXT_Y_OFFSET * scale);

    canvas.draw_string(&label, text_x, text_y, &font, &text_paint);

    Ok(BadgeLayout {
        top_from_bottom: bottom_margin + badge_h,
        left: badge_x,
        right: badge_x + badge_w,
    })
}

pub fn bake_caption(
    canvas: &mut Canvas,
    text: &str,
    typeface: Arc<Typeface>,
    rarity: &dyn Rarity,
    badge: &BadgeLayout,
    scale: f32,
) -> anyhow::Result<()> {
    let h_f = canvas.height() as f32;

    let (font_size, lines) = fit_text(
        text,
        &typeface,
        CAPTION_MAX_WIDTH,
        CAPTION_MAX_BLOCK_HEIGHT * scale,
        CAPTION_MAX_FONT_SIZE * scale,
        CAPTION_MIN_FONT_SIZE * scale,
        scale,
    );

    let font = Font::new(typeface, font_size);
    let stroke_width = CAPTION_STROKE_WIDTH * scale;
    let half_stroke = stroke_width / 2.0;

    let line_xs: Vec<f32> = lines
        .iter()
        .map(|line| (IMAGE_WIDTH - font.measure_text(line)) / 2.0)
        .collect();

    // 1. Calculate the gaps between each adjacent pair of lines
    let mut line_steps = Vec::with_capacity(lines.len().saturating_sub(1));
    for i in 0..lines.len().saturating_sub(1) {
        let collides = lines_collide(&lines[i], line_xs[i], &lines[i + 1], line_xs[i + 1], &font);
        let extra_gap = if collides {
            LINE_COLLISION_EXTRA_SPACING * scale
        } else {
            0.0
        };
        line_steps.push(font_size + (CAPTION_LINE_SPACING * scale) + extra_gap);
    }

    // 2. Check if the bottom line collides with the rarity badge
    let last_idx = lines.len() - 1;
    let badge_collides = descender_collides_with_badge(
        &lines[last_idx],
        line_xs[last_idx],
        &font,
        badge.left,
        badge.right,
    );

    let descender_offset = if badge_collides {
        DESCENDER_AVOID_SHIFT * scale
    } else {
        0.0
    };

    let gap = CAPTION_GAP_ABOVE_BADGE * scale;
    let bottom_baseline = h_f - badge.top_from_bottom - gap - descender_offset - half_stroke;

    // 3. Compute baseline Y positions for all lines working upwards from the bottom line
    let mut y_positions = vec![0.0; lines.len()];
    y_positions[last_idx] = bottom_baseline;
    for i in (0..last_idx).rev() {
        y_positions[i] = y_positions[i + 1] - line_steps[i];
    }

    // 4. Draw stroke outline & fill
    let mut fill = Paint::new();
    fill.set_color(rarity.text_color().into());
    fill.set_anti_alias(true);
    fill.set_style(Style::Fill);

    let mut stroke = Paint::new();
    stroke.set_color(Color::BLACK.into());
    stroke.set_anti_alias(true);
    stroke.set_style(Style::Stroke);
    stroke.set_stroke_width(stroke_width);

    for (i, line) in lines.iter().enumerate() {
        let x = line_xs[i];
        let y = y_positions[i];

        canvas.draw_string(line, x, y, &font, &stroke);
        canvas.draw_string(line, x, y, &font, &fill);
    }

    Ok(())
}

fn fit_text(
    text: &str,
    typeface: &Arc<Typeface>,
    max_width: f32,
    max_height: f32,
    start_size: f32,
    min_size: f32,
    scale: f32,
) -> (f32, Vec<String>) {
    let mut font = Font::new(typeface.clone(), start_size);

    let fits_at = |size: f32, font: &mut Font| -> (bool, Vec<String>) {
        font.set_size(size);
        let lines = word_wrap(text, font, max_width);

        // Crucial: check that EVERY single line fits within the max horizontal width
        let width_fits = lines
            .iter()
            .all(|line| font.measure_text(line) <= max_width);
        if !width_fits {
            return (false, lines);
        }

        if lines.len() <= 1 {
            return (size <= max_height, lines);
        }

        let line_xs: Vec<f32> = lines
            .iter()
            .map(|l| (IMAGE_WIDTH - font.measure_text(l)) / 2.0)
            .collect();

        let mut total_height = size;
        for i in 0..lines.len() - 1 {
            let collides =
                lines_collide(&lines[i], line_xs[i], &lines[i + 1], line_xs[i + 1], font);
            let extra_gap = if collides {
                LINE_COLLISION_EXTRA_SPACING * scale
            } else {
                0.0
            };
            total_height += size + (CAPTION_LINE_SPACING * scale) + extra_gap;
        }

        (total_height <= max_height, lines)
    };

    let (start_fits, start_lines) = fits_at(start_size, &mut font);
    if start_fits {
        return (start_size, start_lines);
    }

    let mut lo = min_size;
    let mut hi = start_size;
    let (_, min_lines) = fits_at(min_size, &mut font);
    let mut best = (min_size, min_lines);

    while hi - lo > 0.5 {
        let mid = (lo + hi) / 2.0;
        let (fits, lines) = fits_at(mid, &mut font);
        if fits {
            best = (mid, lines);
            lo = mid; // Try larger
        } else {
            hi = mid; // Too large, shrink
        }
    }
    best
}

fn word_wrap(text: &str, font: &Font, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::with_capacity(text.len());
    let space_width = font.measure_text(" ");

    for word in text.split_whitespace() {
        let word_width = font.measure_text(word);

        // If a single word alone exceeds max_width, split it across characters
        if word_width > max_width {
            if !current.is_empty() {
                lines.push(current);
                current = String::with_capacity(text.len());
            }
            for ch in word.chars() {
                let mut test = current.clone();
                test.push(ch);
                if font.measure_text(&test) <= max_width {
                    current.push(ch);
                } else {
                    if !current.is_empty() {
                        lines.push(current);
                        current = String::with_capacity(text.len());
                    }
                    current.push(ch);
                }
            }
            continue;
        }

        if current.is_empty() {
            current.push_str(word);
        } else {
            let current_width = font.measure_text(&current);
            if current_width + space_width + word_width <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = String::with_capacity(text.len());
                current.push_str(word);
            }
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
