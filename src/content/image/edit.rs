use std::sync::{Arc, OnceLock};
use std::time::Instant;

use crate::assets::CachedImage;
use crate::content::rarity::{Rarity, RarityManager};
use crate::session::UserSession;
use crate::stats;

use bytes::Bytes;
use skia_rs_canvas::{Canvas, Surface};
use skia_rs_codec::{Image, ImageEncoder, ImageInfo, JpegEncoder, SamplingOptions, decode_image};
use skia_rs_core::{AlphaType, Color, ColorType, IRect, Rect};
use skia_rs_paint::{Paint, Style};
use skia_rs_text::{Font, FontFallback, Typeface};

// =========================================================================
//  TWEAKABLE LAYOUT CONSTANTS (All base values are in pixels at 800px width)
// =========================================================================

pub const IMAGE_WIDTH: f32 = 800.0;
pub const SQUARE_FACTOR: f32 = 0.5;

// --- Rarity Badge Settings ---
pub const RARITY_FONT_SIZE: f32 = 37.0;
pub const RARITY_PAD_X: f32 = 12.0;
pub const RARITY_BADGE_HEIGHT: f32 = 44.0;
pub const RARITY_CORNER_RADIUS: f32 = 8.0;
pub const RARITY_BOTTOM_MARGIN: f32 = 5.0;
pub const RARITY_TEXT_Y_OFFSET: f32 = 35.0; // Distance from badge top to text baseline

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

// Per-channel tolerance when comparing pixel colors (0-255).
const BORDER_COLOR_TOLERANCE: i16 = 8;
// Fraction of pixels in a row/column that must match the reference
// color for that row/column to be considered a solid border.
const BORDER_MIN_MATCH_FRACTION: f32 = 0.98;

// =========================================================================

const MAIN_TTF: &[u8] = include_bytes!("../../../assets/fonts/impact.ttf");
static MAIN_TYPEFACE: OnceLock<Arc<Typeface>> = OnceLock::new();

const BADGE_TTF: &[u8] = include_bytes!("../../../assets/fonts/noto.ttf");
static BADGE_TYPEFACE: OnceLock<Arc<Typeface>> = OnceLock::new();

fn main_typeface() -> anyhow::Result<Arc<Typeface>> {
    if let Some(tf) = MAIN_TYPEFACE.get() {
        return Ok(tf.clone());
    }

    let tf = Arc::new(
        Typeface::from_data(MAIN_TTF.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Cannot initialize Impact font"))?,
    );

    Ok(MAIN_TYPEFACE.get_or_init(|| tf).clone())
}

fn badge_typeface() -> anyhow::Result<Arc<Typeface>> {
    if let Some(tf) = BADGE_TYPEFACE.get() {
        return Ok(tf.clone());
    }

    let tf = Arc::new(
        Typeface::from_data(BADGE_TTF.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Cannot initialize Arial font"))?,
    );

    Ok(BADGE_TYPEFACE.get_or_init(|| tf).clone())
}

#[allow(dead_code)]
fn load_extra_typeface(data: &[u8]) -> Option<Arc<Typeface>> {
    Typeface::from_data(data.to_vec()).map(Arc::new)
}

static FONT_FALLBACK_CHAIN: OnceLock<FontFallback> = OnceLock::new();

fn font_fallback_chain() -> &'static FontFallback {
    FONT_FALLBACK_CHAIN.get_or_init(|| {
        let mut chain = FontFallback::new();

        // Fonts already embedded for other purposes double up as fallback
        // sources for each other.
        for loader in [main_typeface, badge_typeface] {
            if let Ok(tf) = loader() {
                chain.add_fallback(tf);
            }
        }

        // Extra emoji/symbol/CJK fonts get added here - see the module notes
        // above.

        chain
    })
}

fn is_invisible_joiner(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{200D}' | '\u{FE0E}' | '\u{FE0F}')
}

fn resolve_typeface_for_char(
    c: char,
    primary: &Arc<Typeface>,
    fallback: &FontFallback,
) -> Arc<Typeface> {
    if primary.char_to_glyph(c) != 0 {
        return primary.clone();
    }
    fallback.find_font_for_char(c, primary)
}

fn measure_fallback(
    text: &str,
    font_size: f32,
    primary: &Arc<Typeface>,
    fallback: &FontFallback,
) -> f32 {
    let mut cursor = 0.0_f32;
    let mut prev_tf: Option<Arc<Typeface>> = None;

    for ch in text.chars() {
        let tf = if is_invisible_joiner(ch) {
            prev_tf.clone().unwrap_or_else(|| primary.clone())
        } else {
            resolve_typeface_for_char(ch, primary, fallback)
        };
        cursor += Font::new(tf.clone(), font_size).measure_text(&ch.to_string());
        prev_tf = Some(tf);
    }

    cursor
}

struct CharSpan {
    ch: char,
    left: f32,
    right: f32,
    typeface: Arc<Typeface>,
}

/// One consecutive stretch of a line that resolved to the same typeface -
/// the actual unit we draw with a single `canvas.draw_string` call.
struct GlyphRun {
    text: String,
    x: f32,
    typeface: Arc<Typeface>,
}

fn build_runs(spans: &[CharSpan]) -> Vec<GlyphRun> {
    let mut runs: Vec<GlyphRun> = Vec::new();
    for span in spans {
        match runs.last_mut() {
            Some(run) if run.typeface.unique_id() == span.typeface.unique_id() => {
                run.text.push(span.ch);
            }
            _ => runs.push(GlyphRun {
                text: span.ch.to_string(),
                x: span.left,
                typeface: span.typeface.clone(),
            }),
        }
    }
    runs
}

fn run_has_color_glyphs(text: &str, font: &Font) -> bool {
    text.chars().any(|c| {
        let g = font.char_to_glyph(c);
        g != 0 && font.glyph_is_color(g)
    })
}

fn shape_line_relative(
    line: &str,
    font_size: f32,
    primary: &Arc<Typeface>,
    fallback: &FontFallback,
) -> (Vec<CharSpan>, f32) {
    let mut spans = Vec::with_capacity(line.chars().count());
    let mut cursor = 0.0_f32;
    let mut prev_tf: Option<Arc<Typeface>> = None;

    for ch in line.chars() {
        let tf = if is_invisible_joiner(ch) {
            prev_tf.clone().unwrap_or_else(|| primary.clone())
        } else {
            resolve_typeface_for_char(ch, primary, fallback)
        };

        let width = Font::new(tf.clone(), font_size).measure_text(&ch.to_string());

        spans.push(CharSpan {
            ch,
            left: cursor,
            right: cursor + width,
            typeface: tf.clone(),
        });

        cursor += width;
        prev_tf = Some(tf);
    }

    (spans, cursor)
}

/// A line shaped and horizontally centered on the canvas - the shared
/// result reused for collision detection, badge-avoidance, and drawing.
struct ShapedLine {
    spans: Vec<CharSpan>,
    width: f32,
}

fn shape_and_center_line(
    line: &str,
    font_size: f32,
    primary: &Arc<Typeface>,
    fallback: &FontFallback,
) -> ShapedLine {
    let (rel_spans, width) = shape_line_relative(line, font_size, primary, fallback);
    let x = (IMAGE_WIDTH - width) / 2.0;
    let spans = rel_spans
        .into_iter()
        .map(|s| CharSpan {
            left: s.left + x,
            right: s.right + x,
            ..s
        })
        .collect();
    ShapedLine { spans, width }
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

/// Checks if any descender on the top line's spans hangs into the bottom
/// line's spans OR if any top diacritic on the bottom line reaches into the
/// top line.
fn lines_collide(top_spans: &[CharSpan], bottom_spans: &[CharSpan]) -> bool {
    for top in top_spans {
        for bottom in bottom_spans {
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

/// Checks if any descender in the bottom line's spans horizontally overlaps the badge
fn descender_collides_with_badge(spans: &[CharSpan], badge_left: f32, badge_right: f32) -> bool {
    spans
        .iter()
        .any(|s| is_descender(s.ch) && s.right > badge_left && s.left < badge_right)
}

fn colors_match(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(&x, &y)| (x as i16 - y as i16).abs() <= BORDER_COLOR_TOLERANCE)
}

/// Reads pixels and returns the bounding rect of "real" content,
/// stripping away uniform-color borders on any side.
fn detect_content_bounds(image: &Image) -> anyhow::Result<IRect> {
    let w = image.width();
    let h = image.height();

    let info = ImageInfo::new(w, h, ColorType::Rgba8888, AlphaType::Unpremul);
    let row_bytes = w as usize * 4;
    let mut pixels = vec![0u8; row_bytes * h as usize];

    if !image.read_pixels(&info, &mut pixels, row_bytes, 0, 0) {
        anyhow::bail!("Cannot read pixels");
    }

    let get = |x: i32, y: i32| -> [u8; 4] {
        let idx = y as usize * row_bytes + x as usize * 4;
        [
            pixels[idx],
            pixels[idx + 1],
            pixels[idx + 2],
            pixels[idx + 3],
        ]
    };

    let row_is_border = |y: i32| -> bool {
        let reference = get(0, y);
        let matches = (0..w)
            .filter(|&x| colors_match(get(x, y), reference))
            .count();
        matches as f32 / w as f32 >= BORDER_MIN_MATCH_FRACTION
    };

    let col_is_border = |x: i32| -> bool {
        let reference = get(x, 0);
        let matches = (0..h)
            .filter(|&y| colors_match(get(x, y), reference))
            .count();
        matches as f32 / h as f32 >= BORDER_MIN_MATCH_FRACTION
    };

    let mut top = 0;
    while top < h - 1 && row_is_border(top) {
        top += 1;
    }
    let mut bottom = h - 1;
    while bottom > top && row_is_border(bottom) {
        bottom -= 1;
    }

    let mut left = 0;
    while left < w - 1 && col_is_border(left) {
        left += 1;
    }
    let mut right = w - 1;
    while right > left && col_is_border(right) {
        right -= 1;
    }

    // Fallback: if detection collapses to nothing sensible, keep the full image.
    if top >= bottom || left >= right {
        return Ok(IRect::new(0, 0, w, h));
    }

    Ok(IRect::new(left, top, right + 1, bottom + 1))
}

pub fn preprocess_image(image: &Bytes) -> anyhow::Result<Image> {
    let image = decode_image(image)?;

    let full_bounds = IRect::new(0, 0, image.width(), image.height());
    let content_bounds = detect_content_bounds(&image)?;

    let image = if content_bounds != full_bounds {
        image
            .make_subset(&(content_bounds.into()))
            .ok_or_else(|| anyhow::anyhow!("Cannot crop image borders"))?
    } else {
        image
    };

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
    modifiers: String,
    session: UserSession,
) -> anyhow::Result<CachedImage> {
    let start = Instant::now();
    let (w, h) = image.dimensions();

    // 1. Raw text string edit hook
    let text = session.fire_hook("text_string_edit", text);

    let image = if let Some(filter) = rarity.filter() {
        image
            .make_with_filter(filter.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Cannot apply filter"))?
    } else {
        image
    };

    // 2. Base image edit hook (operates directly on Image pixels)
    let image = session.fire_hook("image_edit", image);

    let mut surface = Surface::new_raster_n32_premul(w, h)
        .ok_or_else(|| anyhow::anyhow!("Cannot create surface"))?;
    let mut canvas = surface.canvas();
    canvas.draw_image(&image, 0.0, 0.0, None);

    let scale = h as f32 / IMAGE_WIDTH;

    // 3. Draw the bottom rarity banner
    let arial_tf = badge_typeface()?;
    let badge_layout = bake_rarity_bar(&mut canvas, arial_tf, rarity, scale, modifiers)?;

    // 4. Render caption onto a separate transparent surface
    let impact_tf = main_typeface()?;
    let upper_text = text.to_uppercase();

    let mut caption_surface = Surface::new_raster_n32_premul(w, h)
        .ok_or_else(|| anyhow::anyhow!("Cannot create caption surface"))?;
    bake_caption(
        &mut caption_surface.canvas(),
        &upper_text,
        impact_tf,
        rarity,
        &badge_layout,
        scale,
    )?;

    let caption_image = caption_surface
        .make_image_snapshot()
        .ok_or_else(|| anyhow::anyhow!("Cannot snapshot caption surface"))?;

    // 5. Text image edit hook (operates directly on caption Image pixels)
    let caption_image = session.fire_hook("text_image_edit", caption_image);

    // 6. Draw the corrupted caption onto the main canvas
    canvas.draw_image(&caption_image, 0.0, 0.0, None);

    let baked = surface
        .make_image_snapshot()
        .ok_or_else(|| anyhow::anyhow!("Cannot create snapshot"))?;

    let encode_start = Instant::now();
    let mut out: Vec<u8> = Vec::with_capacity((w * h) as usize / 4);
    JpegEncoder::default().encode(&baked, &mut out)?;

    stats!(
        "Processed image in {:?}\nEncoding: {:?}",
        start.elapsed(),
        encode_start.elapsed(),
    );

    Ok(CachedImage::from_bytes(out))
}

pub struct BadgeLayout {
    pub top_from_bottom: f32,
    pub left: f32,
    pub right: f32,
}

pub fn bake_rarity_bar(
    canvas: &mut Canvas,
    typeface: Arc<Typeface>,
    rarity: &dyn Rarity,
    scale: f32,
    modifiers: String,
) -> anyhow::Result<BadgeLayout> {
    let h_f = canvas.height() as f32;
    let fallback = font_fallback_chain();

    let percentage = RarityManager::global().rarity_percentage(rarity);
    let mod_suffix = if modifiers.trim().is_empty() {
        String::new()
    } else {
        format!(" {}", modifiers.trim())
    };

    let label = format!(
        "{} {} {:.2}%{}",
        rarity.domain_icon(),
        rarity.name(),
        percentage,
        mod_suffix
    );

    let font_size = RARITY_FONT_SIZE * scale;
    let pad_x = RARITY_PAD_X * scale;
    let badge_h = RARITY_BADGE_HEIGHT * scale;
    let corner_radius = RARITY_CORNER_RADIUS * scale;
    let bottom_margin = RARITY_BOTTOM_MARGIN * scale;

    // Fallback-aware measurement: modifier strings aren't guaranteed to be
    // ASCII, so measure (and, further down, draw) the label the same
    // run-by-run way the caption is handled.
    let (_, mut text_width) = shape_line_relative(&label, font_size, &typeface, fallback);
    let mut effective_size = font_size;

    // Ensure badge never overflows the canvas width
    let max_badge_w = IMAGE_WIDTH - 24.0;
    if text_width + pad_x * 2.0 > max_badge_w {
        let fit_font_size = effective_size * (max_badge_w - pad_x * 2.0) / text_width;
        effective_size = fit_font_size.max(12.0);
        let (_, w) = shape_line_relative(&label, effective_size, &typeface, fallback);
        text_width = w;
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

    // Color glyphs (emoji in a modifier string, say) carry their own
    // palette - drawing them with the rarity-colored paint would tint them
    // incorrectly, so they get a neutral, full-alpha paint instead.
    let mut color_glyph_paint = Paint::new();
    color_glyph_paint.set_color(Color::WHITE.into());
    color_glyph_paint.set_anti_alias(true);
    color_glyph_paint.set_style(Style::Fill);

    let text_x = badge_x + pad_x;
    let text_y = badge_y + (RARITY_TEXT_Y_OFFSET * scale);

    let (rel_spans, _) = shape_line_relative(&label, effective_size, &typeface, fallback);
    let abs_spans: Vec<CharSpan> = rel_spans
        .into_iter()
        .map(|s| CharSpan {
            left: s.left + text_x,
            right: s.right + text_x,
            ..s
        })
        .collect();

    for run in build_runs(&abs_spans) {
        let run_font = Font::new(run.typeface.clone(), effective_size);
        if run_has_color_glyphs(&run.text, &run_font) {
            canvas.draw_string(&run.text, run.x, text_y, &run_font, &color_glyph_paint);
        } else {
            canvas.draw_string(&run.text, run.x, text_y, &run_font, &text_paint);
        }
    }

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
    let fallback = font_fallback_chain();

    let (font_size, lines) = fit_text(
        text,
        &typeface,
        CAPTION_MAX_WIDTH,
        CAPTION_MAX_BLOCK_HEIGHT * scale,
        CAPTION_MAX_FONT_SIZE * scale,
        CAPTION_MIN_FONT_SIZE * scale,
        scale,
    );

    let stroke_width = CAPTION_STROKE_WIDTH * scale;
    let half_stroke = stroke_width / 2.0;

    // Shape every line once at the final size; this is reused for collision
    // detection, badge-avoidance, and the glyph runs drawn below, so nothing
    // shifts between what `fit_text` checked and what actually gets drawn.
    let shaped: Vec<ShapedLine> = lines
        .iter()
        .map(|l| shape_and_center_line(l, font_size, &typeface, fallback))
        .collect();

    // 1. Calculate the gaps between each adjacent pair of lines
    let mut line_steps = Vec::with_capacity(shaped.len().saturating_sub(1));
    for i in 0..shaped.len().saturating_sub(1) {
        let collides = lines_collide(&shaped[i].spans, &shaped[i + 1].spans);
        let extra_gap = if collides {
            LINE_COLLISION_EXTRA_SPACING * scale
        } else {
            0.0
        };
        line_steps.push(font_size + (CAPTION_LINE_SPACING * scale) + extra_gap);
    }

    // 2. Check if the bottom line collides with the rarity badge
    let last_idx = shaped.len() - 1;
    let badge_collides =
        descender_collides_with_badge(&shaped[last_idx].spans, badge.left, badge.right);

    let descender_offset = if badge_collides {
        DESCENDER_AVOID_SHIFT * scale
    } else {
        0.0
    };

    let gap = CAPTION_GAP_ABOVE_BADGE * scale;
    let bottom_baseline = h_f - badge.top_from_bottom - gap - descender_offset - half_stroke;

    // 3. Compute baseline Y positions for all lines working upwards from the bottom line
    let mut y_positions = vec![0.0; shaped.len()];
    y_positions[last_idx] = bottom_baseline;
    for i in (0..last_idx).rev() {
        y_positions[i] = y_positions[i + 1] - line_steps[i];
    }

    // 4. Draw stroke outline & fill, one glyph run at a time so that
    // characters outside the Impact font (emoji, Braille, CJK, ...) render
    // from whichever typeface in the Unicode fallback chain actually has
    // them, instead of silently not rendering at all.
    let mut fill = Paint::new();
    fill.set_color(rarity.text_color().into());
    fill.set_anti_alias(true);
    fill.set_style(Style::Fill);

    let mut stroke = Paint::new();
    stroke.set_color(Color::BLACK.into());
    stroke.set_anti_alias(true);
    stroke.set_style(Style::Stroke);
    stroke.set_stroke_width(stroke_width);

    // Color glyphs (emoji) carry their own palette; a black stroke outline
    // or the rarity fill color would look wrong on top of them, so these
    // runs get drawn plain instead of stroke+fill.
    let mut color_glyph_paint = Paint::new();
    color_glyph_paint.set_color(Color::WHITE.into());
    color_glyph_paint.set_anti_alias(true);
    color_glyph_paint.set_style(Style::Fill);

    for (i, line) in shaped.iter().enumerate() {
        let y = y_positions[i];
        for run in build_runs(&line.spans) {
            let run_font = Font::new(run.typeface.clone(), font_size);
            if run_has_color_glyphs(&run.text, &run_font) {
                canvas.draw_string(&run.text, run.x, y, &run_font, &color_glyph_paint);
            } else {
                canvas.draw_string(&run.text, run.x, y, &run_font, &stroke);
                canvas.draw_string(&run.text, run.x, y, &run_font, &fill);
            }
        }
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
    let fallback = font_fallback_chain();

    let fits_at = |size: f32| -> (bool, Vec<String>) {
        let lines = word_wrap(text, size, typeface, fallback, max_width);

        let shaped: Vec<ShapedLine> = lines
            .iter()
            .map(|l| shape_and_center_line(l, size, typeface, fallback))
            .collect();

        // Crucial: check that EVERY single line fits within the max horizontal width
        let width_fits = shaped.iter().all(|s| s.width <= max_width);
        if !width_fits {
            return (false, lines);
        }

        if lines.len() <= 1 {
            return (size <= max_height, lines);
        }

        let mut total_height = size;
        for i in 0..shaped.len() - 1 {
            let collides = lines_collide(&shaped[i].spans, &shaped[i + 1].spans);
            let extra_gap = if collides {
                LINE_COLLISION_EXTRA_SPACING * scale
            } else {
                0.0
            };
            total_height += size + (CAPTION_LINE_SPACING * scale) + extra_gap;
        }

        (total_height <= max_height, lines)
    };

    let (start_fits, start_lines) = fits_at(start_size);
    if start_fits {
        return (start_size, start_lines);
    }

    let mut lo = min_size;
    let mut hi = start_size;
    let (_, min_lines) = fits_at(min_size);
    let mut best = (min_size, min_lines);

    while hi - lo > 0.5 {
        let mid = (lo + hi) / 2.0;
        let (fits, lines) = fits_at(mid);
        if fits {
            best = (mid, lines);
            lo = mid; // Try larger
        } else {
            hi = mid; // Too large, shrink
        }
    }
    best
}

fn word_wrap(
    text: &str,
    font_size: f32,
    primary: &Arc<Typeface>,
    fallback: &FontFallback,
    max_width: f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::with_capacity(text.len());
    let space_width = measure_fallback(" ", font_size, primary, fallback);

    for word in text.split_whitespace() {
        let word_width = measure_fallback(word, font_size, primary, fallback);

        // If a single word alone exceeds max_width, split it across characters
        if word_width > max_width {
            if !current.is_empty() {
                lines.push(current);
                current = String::with_capacity(text.len());
            }
            for ch in word.chars() {
                let mut test = current.clone();
                test.push(ch);
                if measure_fallback(&test, font_size, primary, fallback) <= max_width {
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
            let current_width = measure_fallback(&current, font_size, primary, fallback);
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
