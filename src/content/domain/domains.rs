use super::Domain;
use crate::register_domain;

// =========================================================================
//  1. OCCULT DOMAIN
// =========================================================================

#[derive(Debug)]
pub struct OccultDomain;

impl Domain for OccultDomain {
    fn id(&self) -> &'static str {
        "occult"
    }
    fn name(&self) -> &'static str {
        "Оккультный"
    }
    fn description(&self) -> &'static str {
        "Древние запретные сущности. Направляет резонанс в Токсичные аномалии, подавляя Небесный свет."
    }
    fn resonant_domains(&self) -> &'static [&'static str] {
        &["hazard"]
    }
    fn opposing_domains(&self) -> &'static [&'static str] {
        &["celestial"]
    }
}
register_domain!(OccultDomain);

// =========================================================================
//  2. CELESTIAL DOMAIN
// =========================================================================

#[derive(Debug)]
pub struct CelestialDomain;

impl Domain for CelestialDomain {
    fn id(&self) -> &'static str {
        "celestial"
    }
    fn name(&self) -> &'static str {
        "Небесный"
    }
    fn description(&self) -> &'static str {
        "Высший астральный свет. Направляет резонанс в Кибернетические матрицы, подавляя Оккультную тьму."
    }
    fn resonant_domains(&self) -> &'static [&'static str] {
        &["cybernetic"]
    }
    fn opposing_domains(&self) -> &'static [&'static str] {
        &["occult"]
    }
}
register_domain!(CelestialDomain);

// =========================================================================
//  3. CYBERNETIC DOMAIN
// =========================================================================

#[derive(Debug)]
pub struct CyberneticDomain;

impl Domain for CyberneticDomain {
    fn id(&self) -> &'static str {
        "cybernetic"
    }
    fn name(&self) -> &'static str {
        "Кибернетический"
    }
    fn description(&self) -> &'static str {
        "Синтетические сети и протоколы. Направляет резонанс в Токсичные среды, подавляя Небесную гармонию."
    }
    fn resonant_domains(&self) -> &'static [&'static str] {
        &["hazard"]
    }
    fn opposing_domains(&self) -> &'static [&'static str] {
        &["celestial"]
    }
}
register_domain!(CyberneticDomain);

// =========================================================================
//  4. HAZARD DOMAIN
// =========================================================================

#[derive(Debug)]
pub struct HazardDomain;

impl Domain for HazardDomain {
    fn id(&self) -> &'static str {
        "hazard"
    }
    fn name(&self) -> &'static str {
        "Токсичный"
    }
    fn description(&self) -> &'static str {
        "Радиоактивные и агрессивные мутации. Направляет резонанс в Оккультные бездны, подавляя Кибернетический порядок."
    }
    fn resonant_domains(&self) -> &'static [&'static str] {
        &["occult"]
    }
    fn opposing_domains(&self) -> &'static [&'static str] {
        &["cybernetic"]
    }
}
register_domain!(HazardDomain);

// =========================================================================
//  5. NEUTRAL DOMAIN
// =========================================================================

#[derive(Debug)]
pub struct NeutralDomain;

impl Domain for NeutralDomain {
    fn id(&self) -> &'static str {
        "neutral"
    }
    fn name(&self) -> &'static str {
        "Нейтральный"
    }
    fn description(&self) -> &'static str {
        "Универсальная основа мироздания. Не имеет резонансов и подавлений."
    }
    fn resonant_domains(&self) -> &'static [&'static str] {
        &[]
    }
    fn opposing_domains(&self) -> &'static [&'static str] {
        &[]
    }
}
register_domain!(NeutralDomain);
