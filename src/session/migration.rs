use super::UserProfile;
use crate::content::CatStats;
use std::collections::HashMap;

pub const CURRENT_PROFILE_SCHEMA_VERSION: u32 = 1;

type MigrationFn = fn(&mut UserProfile);

pub struct MigrationManager {
    migrations: Vec<(u32, u32, MigrationFn)>,
}

impl MigrationManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            migrations: Vec::new(),
        };
        mgr.register(0, 1, migrate_v0_to_v1);
        mgr
    }

    pub fn register(&mut self, from_version: u32, to_version: u32, f: MigrationFn) {
        self.migrations.push((from_version, to_version, f));
    }

    pub fn migrate(&self, profile: &mut UserProfile) -> bool {
        let original_version = profile.schema_version;

        if profile.schema_version >= CURRENT_PROFILE_SCHEMA_VERSION {
            return false;
        }

        for (from_v, to_v, migration_fn) in &self.migrations {
            if profile.schema_version == *from_v {
                log::info!(
                    "Migrating profile for user {} from schema v{} -> v{}",
                    profile.user_id,
                    from_v,
                    to_v
                );
                migration_fn(profile);
                profile.schema_version = *to_v;
            }
        }

        profile.schema_version = CURRENT_PROFILE_SCHEMA_VERSION;
        profile.schema_version != original_version
    }
}

fn migrate_v0_to_v1(profile: &mut UserProfile) {
    if profile.shards.is_empty() {
        profile.shards = HashMap::new();
    }

    let clamp_loyalty = |lvl: f32| -> f32 { if lvl < 0.0 { 0.2 } else { lvl.clamp(0.0, 1.0) } };

    if let Some(slot) = profile.inventory.get_mut("equip", 0) {
        slot.cat.loyality.level = clamp_loyalty(slot.cat.loyality.level);
        if slot.cat.stats == CatStats::default() {
            slot.cat.stats = CatStats::roll_random(slot.cat.rarity().tier());
        }
    }

    if let Some(bag) = profile.inventory.category_mut("inventory") {
        for slot in bag.items_mut() {
            slot.cat.loyality.level = clamp_loyalty(slot.cat.loyality.level);
            if slot.cat.stats == CatStats::default() {
                slot.cat.stats = CatStats::roll_random(slot.cat.rarity().tier());
            }
        }
    }
}
