use crate::content::Cat;
use crate::controls::inventory::types::InventorySlot;
use crate::session::UserSession;

pub struct ReleaseResult {
    pub cat: Cat,
    pub shards_awarded: u32,
    pub domain_id: String,
    pub total_shards: u32,
}

pub fn release_cat(
    session: &mut UserSession,
    slot: InventorySlot,
) -> Result<ReleaseResult, String> {
    let (category, idx) = slot.target();
    let Some(taken_slot) = session.inventory.take(category, idx) else {
        return Err("Кот не найден".to_string());
    };

    let cat = taken_slot.cat;
    let shards_awarded = cat.release_shard_reward();
    let domain_id = cat.domain_id().to_string();

    let total = session.shards.entry(domain_id.clone()).or_insert(0);
    *total += shards_awarded;
    let total_shards = *total;

    session.stats.total_released += 1;
    session.fire_trigger("on_cat_release");

    Ok(ReleaseResult {
        cat,
        shards_awarded,
        domain_id,
        total_shards,
    })
}
