use crate::session::UserSession;

pub fn unequip_current(session: &mut UserSession) -> Result<usize, String> {
    if session.inventory.get("equip", 0).is_none() {
        return Err("С собой никого".to_string());
    }

    let target_idx = session
        .inventory
        .category("inventory")
        .and_then(|c| c.first_empty_index())
        .ok_or_else(|| "Все коробки заняты".to_string())?;

    let cat_slot = session
        .inventory
        .take("equip", 0)
        .ok_or_else(|| "Ошибка при перемещении кота".to_string())?;

    match session.inventory.push("inventory", cat_slot) {
        Ok(()) => {
            session.fire_trigger("on_cat_unequip");
            Ok(target_idx)
        }
        Err(returned_slot) => {
            session.inventory.set("equip", 0, returned_slot);
            Err("Все коробки заняты".to_string())
        }
    }
}
