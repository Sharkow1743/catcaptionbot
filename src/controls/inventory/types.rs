use serde::{Deserialize, Serialize};
use teloxide::types::MessageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InventorySlot {
    Equip,
    Bag(usize),
}

impl InventorySlot {
    #[inline]
    pub fn target(self) -> (&'static str, usize) {
        match self {
            Self::Equip => ("equip", 0),
            Self::Bag(idx) => ("inventory", idx),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InventoryTab {
    #[default]
    Overview,
    Stats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryView {
    pub card_msg_id: MessageId,
    pub ctrl_msg_id: MessageId,
    pub selected: InventorySlot,
    pub tab: InventoryTab,
    /// Set while a release is awaiting confirmation. Cleared by any other action.
    pub pending_release: Option<InventorySlot>,
}

impl InventoryView {
    pub fn new(card_msg_id: MessageId, ctrl_msg_id: MessageId, selected: InventorySlot) -> Self {
        Self {
            card_msg_id,
            ctrl_msg_id,
            selected,
            tab: InventoryTab::Overview,
            pending_release: None,
        }
    }
}
