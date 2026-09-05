use crate::session::UserSession;

pub fn unequip_current(session: &mut UserSession) -> Result<usize, String> {
    if session.inventory.get("equip", 0).is_none() {
        return Err("Слот экипировки уже пуст".to_string());
    }

    let has_space = session
        .inventory
        .category("inventory")
        .map(|c| c.has_free_slots())
        .unwrap_or(false);

    if !has_space {
        return Err("В инвентаре нет свободного места!".to_string());
    }

    let cat_slot = session
        .inventory
        .take("equip", 0)
        .ok_or_else(|| "Ошибка при снятии кота".to_string())?;

    match session.inventory.push("inventory", cat_slot) {
        Ok(()) => {
            session.fire_trigger("on_cat_unequip");
            let target_idx = session
                .inventory
                .category("inventory")
                .and_then(|c| c.indexed_items().last().map(|(i, _)| i))
                .unwrap_or(0);
            Ok(target_idx)
        }
        Err(returned_slot) => {
            session.inventory.set("equip", 0, returned_slot);
            Err("В инвентаре нет свободного места!".to_string())
        }
    }
}
