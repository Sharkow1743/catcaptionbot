use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InputMedia, InputMediaPhoto, MessageId,
};

use crate::assets::AssetManager;
use crate::content::Cat;
use crate::content::inventory::Slot;
use crate::session::UserSession;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct CatchContext {
    pub probability: f32,
    pub target_domain_id: &'static str,
    pub target_tier: f32,
    pub time_since_last_roll_ms: i64,
    pub escape_reason: Option<String>,
}

pub fn cat_inline_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[InlineKeyboardButton::callback("Забрать", "take_cat")]])
}

pub async fn handle_take_cat(
    session: &mut UserSession,
    bot: Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    app_state: &AppState,
) {
    let Some(content) = session.last_content.take() else {
        return;
    };

    let target_tier = content.rarity().tier();
    let target_domain_id = content.rarity().domain_id();
    let base_catch = (0.85 - target_tier * 0.20).clamp(0.40, 0.95);
    let time_since_last_roll = session.between_rolls.get_latest().copied().unwrap_or(2000);

    let ctx = CatchContext {
        probability: base_catch,
        target_domain_id,
        target_tier,
        time_since_last_roll_ms: time_since_last_roll,
        escape_reason: None,
    };

    // 1. Calculate catch chance via modifier hooks (Domain perks/debuffs, Loyalty, Adrenaline)
    let evaluated_ctx: CatchContext = session.fire_hook("cat_catch_context", ctx);

    let is_caught = rand::random::<f32>() < evaluated_ctx.probability;

    if !is_caught {
        session.stats.total_escaped += 1;
        session.fire_trigger("on_cat_escape");

        // let reason = evaluated_ctx
        //     .escape_reason
        //     .unwrap_or_else(|| "".to_string());
        let reason = "";

        cat_escape(bot, chat_id, msg_id, &app_state.asset_manager, &reason).await;
        return;
    }

    // 2. Try placing into equipment or general inventory
    let slot = Slot {
        cat: Cat::new(content, app_state).await,
    };
    let placed = match session.inventory.push("equip", slot) {
        Ok(()) => true,
        Err(slot) => session.inventory.push("inventory", slot).is_ok(),
    };

    if placed {
        session.stats.total_caught += 1;
        session.fire_trigger("on_cat_catch");
        let _ = bot
            .edit_message_caption(chat_id, msg_id)
            .caption("Кот теперь ваш!")
            .reply_markup(InlineKeyboardMarkup::default())
            .await;
    } else {
        session.stats.total_escaped += 1;
        session.fire_trigger("on_cat_escape");
        cat_escape(
            bot,
            chat_id,
            msg_id,
            &app_state.asset_manager,
            "Ему не нашлось места",
        )
        .await;
    }
}

pub async fn cat_escape(
    bot: Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    asset_manager: &AssetManager,
    reason: &str,
) {
    let escape_image = asset_manager.require("missing_cat");
    let caption = if reason.is_empty() {
        "Котострофа! Кот убежал!".to_string()
    } else {
        format!("Котострофа! Кот убежал!\n<i>{reason}</i>")
    };

    let photo = InputMediaPhoto::new(escape_image.into())
        .caption(caption)
        .parse_mode(teloxide::types::ParseMode::Html);

    if let Ok(msg_meta) = bot
        .edit_message_media(chat_id, msg_id, InputMedia::Photo(photo))
        .reply_markup(InlineKeyboardMarkup::default())
        .await
    {
        if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
            asset_manager.update_file_id("missing_cat", first_photo.file.id.clone());
        }
    }
}
