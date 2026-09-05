use teloxide::types::MessageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct InventoryView {
    pub msg_id: MessageId,
    pub selected: InventorySlot,
    pub nav_msg_id: Option<MessageId>,
}

pub const EQUIP_LABEL: &str = "Текущий кот";
pub const BACK_LABEL: &str = "Назад";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryAction {
    ViewEquip,
    ViewBag(usize),
    Back,
}

impl InventoryAction {
    pub fn from_text(text: &str) -> Option<Self> {
        match text {
            EQUIP_LABEL => Some(Self::ViewEquip),
            BACK_LABEL => Some(Self::Back),
            other => other
                .chars()
                .next()
                .and_then(|c| c.to_digit(10))
                .map(|n| Self::ViewBag((n as usize).saturating_sub(1))),
        }
    }

    pub fn bag_label(idx: usize, name: &str) -> String {
        format!("{} {}", idx + 1, name)
    }
}
