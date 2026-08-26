#[macro_export]
macro_rules! maybe_kb {
    ($request:expr, $keyboard:expr) => {
        match $keyboard {
            Some(ref kb) => $request.reply_markup(kb.clone()),
            None => $request,
        }
    };
}

#[macro_export]
macro_rules! inline_kb {
    ($([$($text:expr => $data:expr),+ $(,)?]),+ $(,)?) => {
        teloxide::types::InlineKeyboardMarkup::new(vec![
            $(
                vec![
                    $(teloxide::types::InlineKeyboardButton::callback($text, $data)),+
                ]
            ),+
        ])
    };
}

#[macro_export]
macro_rules! reply_kb {
    // Matches rows of [ "Button Text", ... ]
    ($([$($text:expr),+ $(,)?]),+ $(,)?) => {
        teloxide::types::KeyboardMarkup::new(vec![
            $(
                vec![
                    $(teloxide::types::KeyboardButton::new($text)),+
                ]
            ),+
        ])
        .resize_keyboard() // Automatically sizes keyboard cleanly
    };
}
