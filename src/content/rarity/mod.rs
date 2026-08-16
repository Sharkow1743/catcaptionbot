mod rarities;

use crate::text::{Pos, WordSpec};
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;

pub use rarities::*;

pub trait Rarity {
    fn name(&self) -> String;
    fn is_secret(&self) -> bool {
        false
    }
    fn template(&self) -> impl AsRef<[WordSpec]> {
        vec![
            WordSpec::new().poses(&[Pos::Adj]),
            WordSpec::new().poses(&[Pos::Noun]),
        ]
    }
    fn weight(&self) -> f32 {
        1.0
    }
}

pub struct RarityManager<R: Rarity>(Vec<R>);

impl<R: Rarity> RarityManager<R> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn extend<T>(&mut self, rarities: T)
    where
        T: IntoIterator<Item = R>,
    {
        self.0.extend(rarities);
    }

    pub fn add_rarity(&mut self, rarity: R) {
        self.0.push(rarity);
    }

    pub fn pick(&self, rng: &mut impl Rng) -> Option<&R> {
        let weights: Vec<f32> = self.0.iter().map(|i| i.weight()).collect();
        let dist = WeightedIndex::new(weights).ok()?;
        Some(&self.0[dist.sample(rng)])
    }
}
