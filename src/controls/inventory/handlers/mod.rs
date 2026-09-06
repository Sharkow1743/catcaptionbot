mod callback;
mod display;
mod view;

pub use callback::handle_inventory_callback;
pub use display::display_inventory;

use super::types::InventorySlot;
use crate::session::UserSession;

pub(super) fn default_selection(session: &UserSession) -> InventorySlot {
    if session.inventory.get("equip", 0).is_some() {
        InventorySlot::Equip
    } else if let Some(bag) = session.inventory.category("inventory") {
        bag.indexed_items()
            .next()
            .map(|(idx, _)| InventorySlot::Bag(idx))
            .unwrap_or(InventorySlot::Equip)
    } else {
        InventorySlot::Equip
    }
}
