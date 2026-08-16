mod rarities;

use crate::text::{Pos, WordSpec};
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;

pub use rarities::*;
use skia_rs_core::Color;
use skia_rs_paint::ImageFilterRef;

pub trait Rarity {
    fn name(&self) -> String;
    fn is_secret(&self) -> bool {
        false
    }
    fn template(&self) -> Vec<WordSpec> {
        vec![
            WordSpec::new().poses(&[Pos::Adj]),
            WordSpec::new().poses(&[Pos::Noun]),
        ]
    }
    fn weight(&self) -> f32 {
        1.0
    }

    fn filter(&self) -> Option<ImageFilterRef> {
        None
    }

    fn rarity_color(&self) -> Color {
        Color::WHITE
    }

    fn text_color(&self) -> Color {
        Color::WHITE
    }
}

pub struct RarityManager {
    rarities: Vec<Box<dyn Rarity>>,
}

impl RarityManager {
    pub fn new() -> Self {
        Self {
            rarities: Vec::new(),
        }
    }

    // Accept any type that implements Rarity
    pub fn add_rarity(&mut self, rarity: impl Rarity + 'static) {
        self.rarities.push(Box::new(rarity));
    }

    pub fn extend<T>(&mut self, rarities: T)
    where
        T: IntoIterator,
        T::Item: Rarity + 'static,
    {
        for r in rarities {
            self.add_rarity(r);
        }
    }

    pub fn pick(&self) -> Option<&dyn Rarity> {
        let weights: Vec<f32> = self.rarities.iter().map(|i| i.weight()).collect();
        let dist = WeightedIndex::new(weights).ok()?;
        Some(self.rarities[dist.sample(&mut rand::rng())].as_ref())
    }
}
