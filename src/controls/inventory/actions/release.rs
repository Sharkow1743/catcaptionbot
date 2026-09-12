use crate::controls::inventory::types::InventorySlot;
use crate::session::UserSession;

pub fn release_cat(session: &mut UserSession, slot: InventorySlot) -> Result<(), String> {
    let (category, idx) = slot.target();
    if session.inventory.take(category, idx).is_none() {
        return Err("Кот не найден".to_string());
    }

    session.stats.total_released += 1;
    session.fire_trigger("on_cat_release");
    Ok(())
}
