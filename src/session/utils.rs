use std::time::{SystemTime, UNIX_EPOCH};

pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs() as i64
}
