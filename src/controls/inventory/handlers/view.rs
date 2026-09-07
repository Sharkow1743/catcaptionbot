use teloxide::prelude::*;
use teloxide::types::ChatId;

use crate::controls::inventory::renderer::{render_card_in_place, render_controls_in_place};
use crate::controls::inventory::types::InventoryView;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};

/// Re-renders both messages for a new view state and persists it.
pub(super) async fn apply_inventory_view(
    bot: &Bot,
    chat_id: ChatId,
    dialogue: &DialogueState,
    session: &UserSession,
    app_state: &AppState,
    view: InventoryView,
) -> HandlerResult {
    // 1 & 2. Run both renders concurrently in the same task
    let (card_res, ctrl_res) = tokio::join!(
        render_card_in_place(
            bot,
            chat_id,
            view.card_msg_id,
            session,
            &app_state.asset_manager,
            view.selected,
        ),
        render_controls_in_place(
            bot,
            chat_id,
            view.ctrl_msg_id,
            session,
            view.selected,
            view.pending_release,
        )
    );

    card_res?;
    ctrl_res?;

    // 3. Persist session and update dialogue state
    dialogue.update(State::Inventory(view)).await?;

    Ok(())
}
