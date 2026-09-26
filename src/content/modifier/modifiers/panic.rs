use serde::{Deserialize, Serialize};
use skia_rs_canvas::Surface;
use skia_rs_codec::Image;
use std::collections::HashMap;

use crate::content::modifier::{HookArg, Modifier};
use crate::content::rarity::RarityManager;
use crate::controls::take::CatchContext;
use crate::modifier;

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Panic {
    pub level: f32,
}

impl Modifier for Panic {
    const ID: &'static str = "panic";
    const TRIGGERS: &'static [&'static str] = &["on_panic_mashing", "on_roll"];
    const HOOKS: &'static [&'static str] = &[
        "rarity_weights",
        "cat_catch_context",
        "image_edit",
        "text_image_edit",
    ];

    fn is_active(&self) -> bool {
        self.level >= 0.25
    }

    fn display(&self) -> String {
        "⁈".to_string()
    }

    fn apply_hook<'a>(
        &self,
        session: &crate::session::UserSession,
        hook: HookArg<'a>,
    ) -> HookArg<'a> {
        let level = self.level;
        let rarity_mgr = RarityManager::global();

        let panic_resistance = session
            .inventory
            .get_ref("equip", 0)
            .map(|s| s.cat.stats.panic_resistance)
            .unwrap_or(0.0);

        let effective_level = (level * (1.0 - panic_resistance)).max(0.0);

        hook.on::<HashMap<String, f32>>("rarity_weights", move |mut weights| {
            if effective_level < 0.25 {
                return weights;
            }

            for (id, weight) in weights.iter_mut() {
                if let Some(rarity) = rarity_mgr.get_by_id(id) {
                    let tier = rarity.tier();
                    if tier < 0.0 {
                        *weight *= 1.0 + effective_level * 1.5;
                    } else {
                        *weight *= (1.0 - effective_level * 0.75).max(0.05);
                    }
                }
            }
            weights
        })
        .on::<CatchContext>("cat_catch_context", move |mut ctx: CatchContext| {
            if effective_level >= 0.25 {
                ctx.probability = (ctx.probability - effective_level * 0.35).max(0.05);
            }
            ctx
        })
        .on::<Image>("image_edit", move |image: Image| {
            if effective_level < 0.3 {
                return image;
            }

            let (w, h) = image.dimensions();
            let mut surface = match Surface::new_raster_n32_premul(w, h) {
                Some(s) => s,
                None => return image,
            };
            let mut canvas = surface.canvas();
            canvas.draw_image(&image, 0.0, 0.0, None);

            let disp = (effective_level * 12.0) as i32;
            let copy = image.clone();
            canvas.draw_image(&copy, -disp as f32, 0.0, None);
            surface.make_image_snapshot().unwrap_or(image)
        })
        .on::<Image>("text_image_edit", move |caption_img: Image| {
            if effective_level < 0.3 {
                return caption_img;
            }

            let (w, h) = caption_img.dimensions();
            let mut out_surface = match Surface::new_raster_n32_premul(w, h) {
                Some(s) => s,
                None => return caption_img,
            };
            let mut canvas = out_surface.canvas();

            let rand_jitter = || (rand::random::<f32>() * 2.0 - 1.0) * (effective_level * 8.0);
            canvas.draw_image(&caption_img, rand_jitter(), rand_jitter(), None);

            out_surface.make_image_snapshot().unwrap_or(caption_img)
        })
    }

    fn on_trigger(&mut self, name: &str, _session: &mut crate::session::UserSession) {
        match name {
            "on_panic_mashing" => {
                self.level = (self.level + 0.35).min(1.0);
            }
            "on_roll" => {
                self.level = (self.level * 0.65).max(0.0);
            }
            _ => {}
        }
    }
}

modifier!(Panic);
