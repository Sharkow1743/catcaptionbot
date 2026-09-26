use crate::controls::inventory::types::InventorySlot;
use crate::session::UserSession;

pub fn reroll_cat_stats(
    session: &mut UserSession,
    slot: InventorySlot,
) -> Result<(u32, u32), String> {
    let (category, idx) = slot.target();

    // 1. Read domain and cost without holding a long-lived mutable borrow on session.inventory
    let (domain_id, cost) = {
        let Some(cat_slot) = session.inventory.get(category, idx) else {
            return Err("Кот не найден".to_string());
        };
        (
            cat_slot.cat.domain_id().to_string(),
            cat_slot.cat.reroll_shard_cost(),
        )
    };

    // 2. Check and deduct shards from session profile
    let current_shards = session.shards.get(&domain_id).copied().unwrap_or(0);
    if current_shards < cost {
        return Err(format!(
            "Недостаточно осколков (нужно {cost}, есть {current_shards})"
        ));
    }

    *session.shards.entry(domain_id).or_insert(0) -= cost;

    // 3. Mutate the cat's stats in place
    let Some(cat_slot) = session.inventory.get_mut(category, idx) else {
        return Err("Кот не найден".to_string());
    };
    cat_slot.cat.reroll_all_stats();

    Ok((cost, cat_slot.cat.reroll_count))
}
