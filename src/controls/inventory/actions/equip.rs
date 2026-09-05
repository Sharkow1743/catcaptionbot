use crate::session::UserSession;

pub fn equip_from_bag(session: &mut UserSession, idx: usize) -> Result<(), String> {
    let Some(incoming) = session.inventory.take("inventory", idx) else {
        return Err("Выбранный кот не найден".to_string());
    };

    let outgoing = session.inventory.take("equip", 0);
    session.inventory.set("equip", 0, incoming);

    if let Some(outgoing) = outgoing {
        session.inventory.set("inventory", idx, outgoing);
    }

    session.fire_trigger("on_cat_equip");
    Ok(())
}
