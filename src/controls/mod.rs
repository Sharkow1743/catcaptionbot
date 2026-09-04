pub mod inline;
pub mod kb_command;
pub mod roll;
pub mod take;

pub use inline::handle_inline_callback;
pub use kb_command::KbCommand;
pub use roll::handle_keyboard_callback;

use teloxide::types::KeyboardMarkup;

use crate::reply_kb;
use crate::state::State;

impl State {
    pub fn keyboard(&self) -> Option<KeyboardMarkup> {
        let kb = match self {
            State::Start => reply_kb!([KbCommand::Get.to_string()]),
        }
        .persistent()
        .input_field_placeholder(" ");

        if kb.keyboard.is_empty() {
            None
        } else {
            Some(kb)
        }
    }
}
