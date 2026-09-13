use serde::{Deserialize, Serialize};
use skia_rs_canvas::Surface;
use skia_rs_codec::Image;
use skia_rs_core::{Color, Color4f, IRect, Point, Rect};
use skia_rs_paint::RadialGradient;
use skia_rs_paint::{Paint, Style, TileMode};
use std::collections::HashMap;
use std::f32;
use std::sync::Arc;

use crate::content::modifier::{HookArg, Modifier};
use crate::content::rarity::RarityManager;
use crate::controls::take::CatchContext;
use crate::{modifier, notice};

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Adrenaline {
    pub level: f32,
}

impl Modifier for Adrenaline {
    const ID: &'static str = "adrenaline";
    const TRIGGERS: &'static [&'static str] = &["on_roll", "on_cat_catch", "on_cat_escape"];
    const HOOKS: &'static [&'static str] = &[
        "rarity_weights",
        "image_edit",
        "text_image_edit",
        "cat_catch_context",
    ];

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
        session: &crate::session::UserSession,
        hook: HookArg<'a>,
    ) -> HookArg<'a> {
        let level = self.level;
        let rarity_mgr = RarityManager::global();

        let is_titan = session
            .inventory
            .get_ref("equip", 0)
            .map(|s| s.cat.domain_id() == "titan")
            .unwrap_or(false);

        hook.on::<HashMap<String, f32>>("rarity_weights", move |mut weights| {
            if level < 0.5 {
                return weights;
            }

            let intensity = (level - 0.5) * 0.8;
            for (id, weight) in weights.iter_mut() {
                if let Some(rarity) = rarity_mgr.get_by_id(id) {
                    let tier = rarity.tier();
                    let tier_factor = if tier > 0.0 {
                        tier.min(0.65)
                    } else {
                        (-tier).min(0.8)
                    };
                    let mult = 1.0 + intensity * 0.45 * tier_factor;
                    *weight *= mult;
                }
            }
            weights
        })
        .on::<CatchContext>("cat_catch_context", move |mut ctx: CatchContext| {
            if level < 0.5 {
                return ctx;
            }

            // Heavy Titan paws suffer double inertia on high speed
            let mult = if is_titan { 1.8 } else { 1.0 };
            let penalty = (level - 0.4) * 0.22 * mult;
            ctx.probability = (ctx.probability - penalty).max(0.10);

            if is_titan {
                ctx.escape_reason = Some(
                    "Тяжелые монолитные лапы Титана не успели схватить шустрого кота!".to_string(),
                );
            } else {
                ctx.escape_reason =
                    Some("Кот испугался вашей суеты и резких движений!".to_string());
            }

            ctx
        })
        .on::<Image>("image_edit", move |image: Image| {
            if level < 0.5 {
                return image;
            }

            let t = (level * 2.0 - 1.0).max(0.0);
            let intensity = t.powf(1.3);

            let (w, h) = image.dimensions();
            let mut surface = match Surface::new_raster_n32_premul(w, h) {
                Some(s) => s,
                None => return image,
            };
            let mut canvas = surface.canvas();

            canvas.draw_image(&image, 0.0, 0.0, None);

            let cx = w as f32 / 2.0;
            let cy = h as f32 / 2.0;

            canvas.save();
            canvas.translate(cx, cy);
            canvas.scale(cx.max(1.0), cy.max(1.0));

            let max_alpha = (intensity * 210.0).clamp(0.0, 255.0);
            let radius = 1.4142_f32;

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
        .on::<Image>("text_image_edit", move |caption_img: Image| {
            if level < 0.5 {
                return caption_img;
            }

            let t = (level * 2.0 - 1.0).max(0.0);
            let intensity = t.powf(1.4);

            let (w, h) = caption_img.dimensions();
            let mut out_surface = match Surface::new_raster_n32_premul(w, h) {
                Some(s) => s,
                None => return caption_img,
            };
            let mut canvas = out_surface.canvas();

            let rand_bipolar = || rand::random::<f32>() * 2.0 - 1.0;
            let scale = (h as f32 / 800.0).max(0.5);

            if intensity > 0.25 {
                let ghost_count = (1 + (intensity * 1.5).round() as usize).clamp(1, 5);
                let ghost_disp = (2.0 + intensity * 10.0) * scale;
                for _ in 0..ghost_count {
                    let g_dx = rand_bipolar() * ghost_disp;
                    let g_dy = rand_bipolar() * ghost_disp * 0.7;
                    canvas.draw_image(&caption_img, g_dx, g_dy, None);
                }
            }

            canvas.draw_image(&caption_img, 0.0, 0.0, None);

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
                let t = if latest_wait != 0 {
                    (latest_wait as f32 / 1000.0).max(0.0)
                } else {
                    f32::MAX
                };

                let decay_rate = if t > 3.5 {
                    0.32
                } else if t > 1.8 {
                    0.16
                } else {
                    0.08
                };

                let decay_factor = (-decay_rate * t).exp();
                self.level *= decay_factor;

                if t > 15.0 {
                    if self.level <= 1.0 {
                        self.level = 0.0;
                    } else {
                        self.level *= 0.3;
                    }
                }

                let (t_eff, recent_fast_count) = {
                    let history: Vec<f32> = session
                        .between_rolls
                        .iter()
                        .rev()
                        .take(5)
                        .map(|&w| (w as f32 / 1000.0).max(0.0))
                        .collect();

                    if history.is_empty() {
                        (t, 0)
                    } else {
                        let mut weighted_sum = 0.0;
                        let mut total_weight = 0.0;
                        let mut weight = 1.0;
                        let mut fast_count = 0;

                        for &wait in &history {
                            weighted_sum += wait * weight;
                            total_weight += weight;
                            weight *= 0.6;

                            if wait <= 1.8 {
                                fast_count += 1;
                            }
                        }

                        (weighted_sum / total_weight, fast_count)
                    }
                };

                if t <= 2.6 {
                    let speed_factor = ((2.6 - t_eff) / (2.6 - 0.566)).clamp(0.0, 1.0).powf(2.4);
                    let streak_bonus = if recent_fast_count >= 3 { 1.25 } else { 1.0 };
                    let resistance = 1.0 / (1.0 + 2.0 * self.level.powi(2));

                    let domain_bonus = session
                        .inventory
                        .get_ref("equip", 0)
                        .map(|s| {
                            if s.cat.domain_id() == "hazard" {
                                1.35
                            } else {
                                1.0
                            }
                        })
                        .unwrap_or(1.0);

                    let base_impulse = 0.28 * domain_bonus;
                    let gain = base_impulse * speed_factor * resistance * streak_bonus;

                    // Only gain adrenaline if below 1.0, capped at 1.0.
                    // If level > 1.0, it will not gain anything and continues decaying down toward 1.0.
                    if self.level < 1.0 {
                        self.level = (self.level + gain).min(1.0);
                    }
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
                self.level *= 1.3;
                notice!("Adrenaline maintained at {:.2}", self.level);
            }
            _ => {}
        }
    }
}

modifier!(Adrenaline);
