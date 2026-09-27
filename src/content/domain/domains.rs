use super::Domain;
use crate::register_domain;

#[derive(Debug)]
pub struct NeutralDomain;

impl Domain for NeutralDomain {
    fn id(&self) -> &'static str {
        "neutral"
    }
    fn name(&self) -> &'static str {
        "Обыденные"
    }
    fn icon(&self) -> &'static str {
        "○"
    }
}
register_domain!(NeutralDomain);

#[derive(Debug)]
pub struct CyberDomain;

impl Domain for CyberDomain {
    fn id(&self) -> &'static str {
        "cyber"
    }
    fn name(&self) -> &'static str {
        "Кибернетические"
    }
    fn icon(&self) -> &'static str {
        "◈"
    }
    fn primary_resonance(&self) -> Option<&'static str> {
        Some("hazard")
    }
    fn primary_opposing(&self) -> Option<&'static str> {
        Some("occult")
    }
    fn secondary_opposing(&self) -> Option<&'static str> {
        Some("celestial")
    }
    fn take_decay_multiplier(&self, target_domain: &str, _target_tier: f32) -> f32 {
        if target_domain == "occult" || target_domain == "celestial" {
            2.0
        } else {
            1.0
        }
    }
    fn catch_modifier(
        &self,
        target_domain: &str,
        _target_tier: f32,
        _loyalty: f32,
        _time_between_rolls_ms: i64,
    ) -> f32 {
        if target_domain == "hazard" {
            0.20
        } else if target_domain == "occult" || target_domain == "celestial" {
            -0.25
        } else {
            0.05
        }
    }
}
register_domain!(CyberDomain);

#[derive(Debug)]
pub struct HazardDomain;

impl Domain for HazardDomain {
    fn id(&self) -> &'static str {
        "hazard"
    }
    fn name(&self) -> &'static str {
        "Аномальные"
    }
    fn icon(&self) -> &'static str {
        "⌬"
    }
    fn primary_resonance(&self) -> Option<&'static str> {
        Some("titan")
    }
    fn primary_opposing(&self) -> Option<&'static str> {
        Some("cyber")
    }
    fn secondary_opposing(&self) -> Option<&'static str> {
        Some("celestial")
    }
    fn roll_decay_multiplier(&self) -> f32 {
        1.5
    }
    fn catch_modifier(
        &self,
        target_domain: &str,
        _target_tier: f32,
        _loyalty: f32,
        _time_between_rolls_ms: i64,
    ) -> f32 {
        if target_domain == "titan" {
            0.25
        } else if target_domain != "hazard" && target_domain != "occult" {
            -0.15
        } else {
            0.10
        }
    }
}
register_domain!(HazardDomain);

#[derive(Debug)]
pub struct TitanDomain;

impl Domain for TitanDomain {
    fn id(&self) -> &'static str {
        "titan"
    }
    fn name(&self) -> &'static str {
        "Монолитные"
    }
    fn icon(&self) -> &'static str {
        "▲"
    }
    fn primary_resonance(&self) -> Option<&'static str> {
        Some("celestial")
    }
    fn primary_opposing(&self) -> Option<&'static str> {
        Some("hazard")
    }
    fn secondary_opposing(&self) -> Option<&'static str> {
        Some("occult")
    }
    fn roll_decay_multiplier(&self) -> f32 {
        0.5
    }
    fn take_decay_multiplier(&self, _target_domain: &str, target_tier: f32) -> f32 {
        if target_tier <= 0.0 { 0.0 } else { 1.0 }
    }
    fn catch_modifier(
        &self,
        target_domain: &str,
        _target_tier: f32,
        _loyalty: f32,
        _time_between_rolls_ms: i64,
    ) -> f32 {
        if target_domain == "celestial" {
            0.20
        } else if target_domain == "hazard" {
            -0.20
        } else {
            0.05
        }
    }
}
register_domain!(TitanDomain);

#[derive(Debug)]
pub struct CelestialDomain;

impl Domain for CelestialDomain {
    fn id(&self) -> &'static str {
        "celestial"
    }
    fn name(&self) -> &'static str {
        "Небесные"
    }
    fn icon(&self) -> &'static str {
        "☼"
    }
    fn primary_resonance(&self) -> Option<&'static str> {
        Some("occult")
    }
    fn primary_opposing(&self) -> Option<&'static str> {
        Some("titan")
    }
    fn secondary_opposing(&self) -> Option<&'static str> {
        Some("cyber")
    }
    fn passive_regen_multiplier(&self) -> f32 {
        1.5
    }
    fn take_decay_multiplier(&self, _target_domain: &str, target_tier: f32) -> f32 {
        if target_tier < 0.0 { 2.0 } else { 0.8 }
    }
    fn catch_modifier(
        &self,
        target_domain: &str,
        _target_tier: f32,
        _loyalty: f32,
        _time_between_rolls_ms: i64,
    ) -> f32 {
        if target_domain == "occult" {
            0.25
        } else if target_domain == "titan" {
            -0.20
        } else {
            0.10
        }
    }
}
register_domain!(CelestialDomain);

#[derive(Debug)]
pub struct OccultDomain;

impl Domain for OccultDomain {
    fn id(&self) -> &'static str {
        "occult"
    }
    fn name(&self) -> &'static str {
        "Оккультные"
    }
    fn icon(&self) -> &'static str {
        "✦"
    }
    fn primary_resonance(&self) -> Option<&'static str> {
        Some("cyber")
    }
    fn primary_opposing(&self) -> Option<&'static str> {
        Some("celestial")
    }
    fn secondary_opposing(&self) -> Option<&'static str> {
        Some("hazard")
    }
    fn release_boost_multiplier(&self) -> f32 {
        2.0
    }
}
register_domain!(OccultDomain);
