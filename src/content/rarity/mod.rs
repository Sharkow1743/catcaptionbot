mod rarities;
use crate::text::{Pos, WordSpec};
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
pub use rarities::*;
use skia_rs_core::Color;
use skia_rs_paint::ImageFilterRef;
use std::sync::{Arc, OnceLock};

// Re-exported so the macros below work from any module without callers
// needing `inventory` as a direct dependency.
pub use inventory;

pub trait Rarity: Send + Sync {
    fn name(&self) -> String;
    fn id(&self) -> String;
    fn is_secret(&self) -> bool {
        false
    }
    fn template(&self) -> Vec<WordSpec> {
        vec![
            WordSpec::new().poses(&[Pos::Adj]),
            WordSpec::new().poses(&[Pos::Noun]),
        ]
    }
    fn template_failable(&self) -> anyhow::Result<Vec<WordSpec>> {
        Ok(self.template())
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

/// A single entry submitted into the global rarity registry.
pub struct RarityRegistration(pub fn() -> Arc<dyn Rarity>);
inventory::collect!(RarityRegistration);

#[macro_export]
macro_rules! rarity_struct {
    ($vis:vis $Name:ident, id = $id:literal, name = $name:literal $(, $($rest:tt)*)?) => {
        $vis struct $Name;

        impl $crate::rarity::Rarity for $Name {
            fn id(&self) -> String { $id.to_string() }
            fn name(&self) -> String { $name.to_string() }
            $($($rest)*)?
        }

        $crate::rarity::inventory::submit! {
            $crate::rarity::RarityRegistration(|| std::sync::Arc::new($Name))
        }
    };
}

#[macro_export]
macro_rules! register_rarity {
    ($expr:expr) => {
        $crate::content::rarity::inventory::submit! {
            $crate::content::rarity::RarityRegistration(|| std::sync::Arc::new($expr))
        }
    };
}

#[derive(Clone)]
pub struct RarityManager {
    rarities: Vec<Arc<dyn Rarity>>,
}

impl RarityManager {
    pub fn new() -> Self {
        Self {
            rarities: Vec::new(),
        }
    }

    pub fn add_rarity(&mut self, rarity: impl Rarity + 'static) {
        self.rarities.push(Arc::new(rarity));
    }

    pub fn extend<T>(mut self, rarities: T) -> Self
    where
        T: IntoIterator,
        T::Item: Rarity + 'static,
    {
        for r in rarities {
            self.add_rarity(r);
        }
        self
    }

    pub fn from_registry() -> Self {
        let mut mgr = Self::new();
        for reg in inventory::iter::<RarityRegistration> {
            mgr.rarities.push((reg.0)());
        }
        mgr
    }

    pub fn global() -> &'static RarityManager {
        static GLOBAL: OnceLock<RarityManager> = OnceLock::new();
        GLOBAL.get_or_init(RarityManager::from_registry)
    }

    pub fn total_weight(&self) -> f32 {
        self.rarities.iter().map(|r| r.weight()).sum()
    }

    pub fn rarity_percentage(&self, rarity: &dyn Rarity) -> f32 {
        let total = self.total_weight();
        if total > 0.0 {
            (rarity.weight() / total) * 100.0
        } else {
            0.0
        }
    }

    pub fn pick(&self) -> Option<Arc<dyn Rarity>> {
        let weights: Vec<f32> = self.rarities.iter().map(|i| i.weight()).collect();
        let dist = WeightedIndex::new(weights).ok()?;
        Some(self.rarities[dist.sample(&mut rand::rng())].clone())
    }
}

pub struct SimpleRarity {
    name: String,
    id: String,
    is_secret: bool,
    template: Vec<WordSpec>,
    weight: f32,
    filter: Option<ImageFilterRef>,
    rarity_color: Color,
    text_color: Color,
}

impl SimpleRarity {
    pub fn new(name: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: id.into(),
            is_secret: false,
            template: Vec::new(),
            weight: 1.0,
            filter: None,
            rarity_color: Color::WHITE,
            text_color: Color::WHITE,
        }
    }
    pub fn secret(mut self, v: bool) -> Self {
        self.is_secret = v;
        self
    }
    pub fn weight(mut self, w: f32) -> Self {
        self.weight = w;
        self
    }
    pub fn template(mut self, t: Vec<WordSpec>) -> Self {
        self.template = t;
        self
    }
    pub fn filter(mut self, f: ImageFilterRef) -> Self {
        self.filter = Some(f);
        self
    }
    pub fn rarity_color(mut self, c: Color) -> Self {
        self.rarity_color = c;
        self
    }
    pub fn text_color(mut self, c: Color) -> Self {
        self.text_color = c;
        self
    }
}

impl Rarity for SimpleRarity {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn id(&self) -> String {
        self.id.clone()
    }
    fn is_secret(&self) -> bool {
        self.is_secret
    }
    fn template(&self) -> Vec<WordSpec> {
        self.template.clone()
    }
    fn weight(&self) -> f32 {
        self.weight
    }
    fn filter(&self) -> Option<ImageFilterRef> {
        self.filter.clone()
    }
    fn rarity_color(&self) -> Color {
        self.rarity_color
    }
    fn text_color(&self) -> Color {
        self.text_color
    }
}
