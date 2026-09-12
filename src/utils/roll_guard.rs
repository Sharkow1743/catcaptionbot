use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static ACTIVE_ROLLS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

fn active_rolls() -> &'static Mutex<HashSet<u64>> {
    ACTIVE_ROLLS.get_or_init(|| Mutex::new(HashSet::new()))
}

// RAII guard that automatically releases the lock when serve_cat_roll exits
pub struct RollGuard(u64);

impl RollGuard {
    pub fn try_acquire(user_id: u64) -> Option<Self> {
        let mut set = active_rolls().lock().unwrap();
        if set.insert(user_id) {
            Some(Self(user_id))
        } else {
            None
        }
    }
}

impl Drop for RollGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = active_rolls().lock() {
            set.remove(&self.0);
        }
    }
}
