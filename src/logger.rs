use chrono::Local;
use log::{Level, LevelFilter as LogLevelFilter};
use std::io::Write;
use std::sync::Mutex;

// ─── Custom Level Hierarchy & Filter ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FilterLevel {
    Off = 0,
    Error = 1,
    Warn = 2,
    Notice = 3,
    Info = 4,
    Detail = 5, // Both DETAL and STATS share rank 5
    Debug = 6,
    Trace = 7,
}

pub use FilterLevel::Detail as Stats;

impl FilterLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "off" => FilterLevel::Off,
            "error" => FilterLevel::Error,
            "warn" | "warning" => FilterLevel::Warn,
            "notice" => FilterLevel::Notice,
            "info" => FilterLevel::Info,
            "detail" | "stats" | "verbose" => FilterLevel::Detail,
            "debug" => FilterLevel::Debug,
            "trace" => FilterLevel::Trace,
            _ => FilterLevel::Info,
        }
    }

    pub fn to_log_filter(self) -> LogLevelFilter {
        match self {
            FilterLevel::Off => LogLevelFilter::Off,
            FilterLevel::Error => LogLevelFilter::Error,
            FilterLevel::Warn => LogLevelFilter::Warn,
            FilterLevel::Notice | FilterLevel::Info => LogLevelFilter::Info,
            FilterLevel::Detail | FilterLevel::Debug => LogLevelFilter::Debug,
            FilterLevel::Trace => LogLevelFilter::Trace,
        }
    }
}

// ─── Custom Level Macros ─────────────────────────────────────────────────────

/// Notice level (between WARN and INFO) - Purple/Magenta
#[macro_export]
macro_rules! notice {
    ($($arg:tt)+) => {
        log::info!(target: concat!(module_path!(), "::__lvl:NOTIC"), $($arg)+)
    };
}

/// Detail level (between INFO and DEBUG) - Dimmed Cyan
#[macro_export]
macro_rules! detail {
    ($($arg:tt)+) => {
        log::debug!(target: concat!(module_path!(), "::__lvl:DETAL"), $($arg)+)
    };
}

/// Stats level (between INFO and DEBUG) - Bright Blue / Steel
#[macro_export]
macro_rules! stats {
    ($($arg:tt)+) => {
        log::debug!(target: concat!(module_path!(), "::__lvl:STATS"), $($arg)+)
    };
}

// ─────────────────────────────────────────────────────────────────────────────

fn get_terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(100)
}

fn wrap_text(line: &str, first_max: usize, wrap_max: usize) -> Vec<String> {
    let first_max = first_max.max(15);
    let wrap_max = wrap_max.max(15);

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut max_len = first_max;

    for word in line.split(' ') {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= max_len {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
            max_len = wrap_max;
        }
    }

    lines.push(current);
    lines
}

pub struct LoggerBuilder {
    app_filter: FilterLevel,
    external_filter: FilterLevel,
}

impl LoggerBuilder {
    pub fn new() -> Self {
        Self {
            app_filter: FilterLevel::Detail,
            external_filter: FilterLevel::Warn,
        }
    }

    pub fn app(mut self, filter: FilterLevel) -> Self {
        self.app_filter = filter;
        self
    }

    pub fn external(mut self, filter: FilterLevel) -> Self {
        self.external_filter = filter;
        self
    }

    pub fn all(mut self, filter: FilterLevel) -> Self {
        self.app_filter = filter;
        self.external_filter = filter;
        self
    }

    pub fn init(self) {
        let mut builder = env_logger::Builder::from_default_env();
        let crate_name = env!("CARGO_PKG_NAME").replace('-', "_");

        if std::env::var("RUST_LOG").is_err() {
            builder
                .filter_level(self.external_filter.to_log_filter())
                .filter_module(&crate_name, self.app_filter.to_log_filter());
        }

        let app_filter = self.app_filter;
        let external_filter = self.external_filter;
        let last_seen = Mutex::new((Option::<String>::None, Option::<String>::None));
        let crate_prefix = format!("{crate_name}::");

        builder.format(move |f, record| {
            // 1. Parse module path and custom level tag
            let raw_target = record.target();
            let (actual_target, custom_lvl) = if let Some((mod_path, lvl)) = raw_target.split_once("::__lvl:") {
                (mod_path, Some(lvl))
            } else {
                (raw_target, None)
            };

            // 2. Identify if target belongs to this crate
            let is_app = actual_target == crate_name || actual_target.starts_with(&crate_prefix);
            let active_filter = if is_app { app_filter } else { external_filter };

            // 3. Map record to FilterLevel hierarchy
            let record_filter_level = if let Some(lvl) = custom_lvl {
                match lvl {
                    "NOTIC" => FilterLevel::Notice,
                    "DETAL" | "STATS" => FilterLevel::Detail,
                    _ => FilterLevel::Info,
                }
            } else {
                match record.level() {
                    Level::Error => FilterLevel::Error,
                    Level::Warn => FilterLevel::Warn,
                    Level::Info => FilterLevel::Info,
                    Level::Debug => FilterLevel::Debug,
                    Level::Trace => FilterLevel::Trace,
                }
            };

            if record_filter_level > active_filter {
                return Ok(());
            }

            let now = Local::now();
            let cur_date = now.format("%Y-%m-%d").to_string();
            let cur_time = now.format("%H:%M:%S").to_string();

            let (date_changed, time_changed) = {
                let mut guard = last_seen.lock().unwrap_or_else(|p| p.into_inner());
                let d_changed = guard.0.as_deref() != Some(&cur_date);
                let t_changed = guard.1.as_deref() != Some(&cur_time);

                if d_changed {
                    guard.0 = Some(cur_date.clone());
                }
                if t_changed {
                    guard.1 = Some(cur_time.clone());
                }

                (d_changed, t_changed)
            };

            if date_changed {
                writeln!(f, "\x1b[2m─── {} ───────────────────────────────────────────────────────────────────────────\x1b[0m", cur_date)?;
            }

            let time_plain = if time_changed { &cur_time } else { "        " };
            let time_styled = format!("\x1b[2m{time_plain}\x1b[0m");

            let (level_plain, level_styled) = if let Some(lvl) = custom_lvl {
                match lvl {
                    "NOTIC" => ("NOTIC", "\x1b[35mNOTIC\x1b[0m"),     // Purple
                    "DETAL" => ("DETAL", "\x1b[2;36mDETAL\x1b[0m"),   // Dimmed Cyan
                    "STATS" => ("STATS", "\x1b[94mSTATS\x1b[0m"),     // Bright Blue / Steel
                    other => (other, other),
                }
            } else {
                match record.level() {
                    Level::Trace => ("TRACE", "\x1b[36mTRACE\x1b[0m"), // Cyan
                    Level::Debug => ("DEBUG", "\x1b[34mDEBUG\x1b[0m"), // Blue
                    Level::Info  => (" INFO", "\x1b[32m INFO\x1b[0m"), // Green
                    Level::Warn  => (" WARN", "\x1b[33m WARN\x1b[0m"), // Yellow
                    Level::Error => ("ERROR", "\x1b[31mERROR\x1b[0m"), // Red
                }
            };

            let plain_prefix = format!("{time_plain} {level_plain} > ");
            let styled_prefix = format!("{time_styled} {level_styled} > ");

            let target = if let Some(stripped) = actual_target.strip_prefix(&crate_prefix) {
                stripped
            } else if actual_target == crate_name {
                "app"
            } else {
                actual_target
            };
            let module_plain = format!("[{target}]");
            let module_styled = format!("\x1b[2m[{target}]\x1b[0m");

            let term_width = get_terminal_width();
            let base_len = plain_prefix.len();
            let newline_len = base_len + 1;
            let wrap_len = base_len + 4;

            let newline_indent = " ".repeat(newline_len);
            let wrap_indent = " ".repeat(wrap_len);

            let first_line_max = term_width.saturating_sub(base_len + module_plain.len() + 2);
            let newline_first_max = term_width.saturating_sub(newline_len);
            let wrap_max = term_width.saturating_sub(wrap_len);

            let message = record.args().to_string();
            let mut is_first_record_line = true;

            for raw_line in message.lines() {
                let line_max = if is_first_record_line { first_line_max } else { newline_first_max };
                let wrapped = wrap_text(raw_line, line_max, wrap_max);

                for (sub_idx, subline) in wrapped.into_iter().enumerate() {
                    if is_first_record_line {
                        let used_width = base_len + subline.len() + module_plain.len();
                        let padding_len = term_width.saturating_sub(used_width).max(1);
                        let padding = " ".repeat(padding_len);

                        writeln!(f, "{}{}{}{}", styled_prefix, subline, padding, module_styled)?;
                        is_first_record_line = false;
                    } else if sub_idx == 0 {
                        writeln!(f, "{}{}", newline_indent, subline)?;
                    } else {
                        writeln!(f, "{}{}", wrap_indent, subline)?;
                    }
                }
            }

            Ok(())
        });

        builder.init();
    }
}

pub fn builder() -> LoggerBuilder {
    LoggerBuilder::new()
}

pub fn init() {
    LoggerBuilder::new().init();
}
