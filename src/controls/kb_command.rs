use std::str::FromStr;
use strum::{Display, EnumIter, EnumString};
use teloxide::dispatching::DpHandlerDescription;
use teloxide::prelude::*;
use teloxide::types::{Message, Update};

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display, EnumIter)]
#[strum(ascii_case_insensitive)]
pub enum KbCommand {
    #[strum(serialize = "get", to_string = "Получить")]
    Get,
    #[strum(serialize = "inventory", to_string = "Мои коты")]
    Inventory,
    #[strum(serialize = "battles", to_string = "Дуэли")]
    Battles,
    #[strum(serialize = "stats", to_string = "Статистика")]
    Stats,
    #[strum(serialize = "random_battle", to_string = "Случайная дуэль")]
    RandomBattle,
    #[strum(serialize = "invite_battle", to_string = "Позвать друга")]
    InviteBattle,
    #[strum(serialize = "cancel_queue", to_string = "Отменить поиск")]
    CancelQueue,
    #[strum(serialize = "surrender", to_string = "Сдаться")]
    Surrender,
    #[strum(serialize = "back", to_string = "В главное меню")]
    Back,
}

impl KbCommand {
    pub fn from_display_text(text: &str) -> Option<Self> {
        use strum::IntoEnumIterator;
        Self::iter().find(|cmd| cmd.to_string() == text)
    }

    #[allow(dead_code)]
    pub fn from_id(id: &str) -> Option<Self> {
        <Self as FromStr>::from_str(id).ok()
    }

    #[allow(dead_code)]
    pub fn extract_from_message(msg: Message) -> Option<Self> {
        msg.text().and_then(Self::from_display_text)
    }
}

pub trait UpdateKbExt {
    fn filter_kb_callback<Output>() -> dptree::Handler<'static, Output, DpHandlerDescription>
    where
        Output: Send + Sync + 'static;
}

impl UpdateKbExt for Update {
    fn filter_kb_callback<Output>() -> dptree::Handler<'static, Output, DpHandlerDescription>
    where
        Output: Send + Sync + 'static,
    {
        Update::filter_message()
            .filter_map(|msg: Message| msg.text().and_then(KbCommand::from_display_text))
    }
}
