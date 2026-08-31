use serde::{Deserialize, Serialize};
use skia_rs_canvas::Surface;
use skia_rs_codec::Image;
use skia_rs_core::{Color, Color4f, IRect, Point, Rect};
use skia_rs_paint::RadialGradient;
use skia_rs_paint::{Paint, Style, TileMode};
use std::sync::Arc;

use crate::content::modifier::{HookArg, Modifier};
use crate::{modifier, notice};

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Adrenaline {
    pub level: f32,
}

impl Modifier for Adrenaline {
    const ID: &'static str = "adrenaline";
    const TRIGGERS: &'static [&'static str] = &["on_roll", "on_cat_catch", "on_cat_escape"];
    const HOOKS: &'static [&'static str] = &["rarity_weights", "image_edit", "text_image_edit"];

    fn is_active(&self) -> bool {
        self.level >= 0.5
    }

    fn display(&self) -> String {
        if self.level > 1.0 {
            "⚡⚡".to_string()
        } else {
            "⚡".to_string()
        }
    }

    fn apply_hook<'a>(
        &self,
        _session: &crate::session::UserSession,
        hook: HookArg<'a>,
    ) -> HookArg<'a> {
        hook.on::<Image>("image_edit", |image: Image| {
            if self.level < 0.5 {
                return image;
            }

            // Uncapped visual scaling
            let t = (self.level * 2.0 - 1.0).max(0.0);
            let intensity = t.powf(1.3);

            let (w, h) = image.dimensions();
            let mut surface = match Surface::new_raster_n32_premul(w, h) {
                Some(s) => s,
                None => return image,
            };
            let mut canvas = surface.canvas();

            // 1. Base image
            canvas.draw_image(&image, 0.0, 0.0, None);

            // 2. Elliptical vignette
            let cx = w as f32 / 2.0;
            let cy = h as f32 / 2.0;

            canvas.save();
            canvas.translate(cx, cy);
            canvas.scale(cx.max(1.0), cy.max(1.0));

            let max_alpha = (intensity * 210.0).clamp(0.0, 255.0);
            let radius = 1.4142_f32;

            // Darkness creeps inward as adrenaline exceeds 1.0
            let start = (0.30 - (intensity - 1.0).max(0.0) * 0.15).max(0.0);

            let steps = 10;
            let mut colors: Vec<Color4f> = Vec::with_capacity(steps);
            let mut stops: Vec<f32> = Vec::with_capacity(steps);

            for i in 0..steps {
                let stop = i as f32 / (steps - 1) as f32;
                stops.push(stop);

                let factor = if stop <= start {
                    0.0
                } else {
                    let norm = (stop - start) / (1.0 - start).max(0.01);
                    norm * norm
                };

                let alpha = (factor * max_alpha).round() as u8;
                colors.push(Color::from_argb(alpha, 190, 0, 10).into());
            }

            let shader = RadialGradient::new(
                Point::new(0.0, 0.0),
                radius,
                colors,
                Some(stops),
                TileMode::Clamp,
            );

            let mut paint = Paint::new();
            paint.set_shader(Some(Arc::new(shader)));
            paint.set_anti_alias(true);
            paint.set_style(Style::Fill);
            canvas.draw_rect(&Rect::from_xywh(-1.0, -1.0, 2.0, 2.0), &paint);

            canvas.restore();

            surface.make_image_snapshot().unwrap_or(image)
        })
        .on::<Image>("text_image_edit", |caption_img: Image| {
            if self.level < 0.5 {
                return caption_img;
            }

            let t = (self.level * 2.0 - 1.0).max(0.0);
            let intensity = t.powf(1.4);

            let (w, h) = caption_img.dimensions();
            let mut out_surface = match Surface::new_raster_n32_premul(w, h) {
                Some(s) => s,
                None => return caption_img,
            };
            let mut canvas = out_surface.canvas();

            let rand_bipolar = || rand::random::<f32>() * 2.0 - 1.0;
            let scale = (h as f32 / 800.0).max(0.5);

            // 1. Ghost double-vision passes
            if intensity > 0.25 {
                let ghost_count = (1 + (intensity * 1.5).round() as usize).clamp(1, 5);
                let ghost_disp = (2.0 + intensity * 10.0) * scale;
                for _ in 0..ghost_count {
                    let g_dx = rand_bipolar() * ghost_disp;
                    let g_dy = rand_bipolar() * ghost_disp * 0.7;
                    canvas.draw_image(&caption_img, g_dx, g_dy, None);
                }
            }

            // 2. Base crisp text
            canvas.draw_image(&caption_img, 0.0, 0.0, None);

            // 3. Per-character jitter
            let base_char_width = (42.0 * scale).max(6.0);
            let max_disp_x = (1.5 + intensity * 12.0) * scale;
            let max_disp_y = (0.8 + intensity * 7.0) * scale;

            let mut x = 0;
            while x < w {
                let col_mult = 0.6 + rand::random::<f32>() * 0.8;
                let col_width = ((base_char_width * col_mult).round() as i32).max(4);
                let slice_w = col_width.min(w - x);
                let rect = IRect::new(x, 0, x + slice_w, h);

                if let Some(subset) = caption_img.make_subset(&(rect.into())) {
                    let mut dx = rand_bipolar() * max_disp_x;
                    let mut dy = rand_bipolar() * max_disp_y;

                    if intensity > 0.6 && rand::random::<f32>() < (intensity * 0.4).min(0.9) {
                        dx *= 2.5;
                        dy *= 2.2;
                    }

                    canvas.draw_image(&subset, x as f32 + dx, dy, None);
                }

                x += slice_w;
            }

            // 4. Horizontal glitch tearing
            if intensity > 0.35 {
                let num_tears = ((intensity * 12.0).round() as usize).max(1);
                let tear_max_disp = (5.0 + intensity * 30.0) * scale;

                for _ in 0..num_tears {
                    let band_h = (rand::random::<f32>() * 25.0 * scale).max(2.0) as i32;
                    let max_y = (h - band_h).max(0);
                    if max_y == 0 {
                        continue;
                    }
                    let y = (rand::random::<f32>() * max_y as f32) as i32;
                    let rect = IRect::new(0, y, w, y + band_h);

                    if let Some(subset) = caption_img.make_subset(&(rect.into())) {
                        let tear_dx = rand_bipolar() * tear_max_disp;
                        let tear_dy = rand_bipolar() * (3.0 * scale);
                        canvas.draw_image(&subset, tear_dx, y as f32 + tear_dy, None);
                    }
                }
            }

            out_surface.make_image_snapshot().unwrap_or(caption_img)
        })
    }

    fn on_trigger(&mut self, name: &str, session: &mut crate::session::UserSession) {
        match name {
            "on_roll" => {
                let latest_wait = session.between_rolls.get_latest().copied().unwrap_or(0);
                let t = (latest_wait as f32 / 1000.0).max(0.0);

                // --- 1. CONTINUOUS NATURAL TIME DECAY ---
                let decay_rate = if t > 3.5 {
                    0.32 // Rapid dissipation if pausing / reading
                } else if t > 1.8 {
                    0.16 // Solid drag for normal casual play (1.8s - 2.5s drops you out of adrenaline)
                } else {
                    0.08 // Base drag during fast mashing
                };

                let decay_factor = (-decay_rate * t).exp();
                self.level *= decay_factor;

                if t > 15.0 {
                    if self.level <= 1.0 {
                        self.level = 0.0;
                    } else {
                        self.level *= 0.3; // Soft drop for un-capped external buffs
                    }
                }

                // --- 2. SPEED-BASED IMPULSE GAIN ---
                // Calibrated to 0.566s engine minimum & cut off at 2.6s
                if t <= 2.6 {
                    // Steep non-linear impulse curve:
                    // ~0.57s -> 1.00  (Machine limit -> full gain)
                    // ~1.00s -> 0.56  (Tryhard mash -> strong gain, pushes 0.7 - 0.9)
                    // ~1.50s -> 0.22  (Active edge -> balances around ~0.40)
                    // ~2.00s -> 0.05  (Normal casual -> decays down to ~0.20)
                    // >=2.60s -> 0.00 (Zero gain)
                    let speed_factor = ((2.6 - t) / (2.6 - 0.566)).clamp(0.0, 1.0).powf(2.4);

                    // Resistance: Exponential metabolic resistance to gaining more
                    let resistance = 1.0 / (1.0 + 2.0 * self.level.powi(2));

                    let base_impulse = 0.28;
                    let gain = base_impulse * speed_factor * resistance;

                    self.level = (self.level + gain).max(0.0);
                }

                if self.level < 0.01 {
                    self.level = 0.0;
                }

                notice!("Adrenaline updated to {:.2}", self.level);
            }
            "on_cat_catch" => {
                self.level = (self.level * 0.50).max(0.0);
                notice!("Adrenaline reduced to {:.2}", self.level);
            }
            "on_cat_escape" => {
                self.level = self.level * 1.5;
                notice!("Adrenaline maintained at {:.2}", self.level);
            }
            _ => {}
        }
    }
}

modifier!(Adrenaline);
