use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

static ACTIVE_ROLLS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
static SPAM_COUNTERS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();

fn active_rolls() -> &'static Mutex<HashSet<u64>> {
    ACTIVE_ROLLS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn spam_counters() -> &'static Mutex<HashMap<u64, usize>> {
    SPAM_COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct RollGuard(u64);

impl RollGuard {
    pub fn try_acquire(user_id: u64) -> Option<Self> {
        let mut set = active_rolls().lock().unwrap();
        if set.insert(user_id) {
            Some(Self(user_id))
        } else {
            let mut spams = spam_counters().lock().unwrap();
            *spams.entry(user_id).or_insert(0) += 1;
            None
        }
    }

    pub fn take_spam_count(user_id: u64) -> usize {
        let mut spams = spam_counters().lock().unwrap();
        spams.remove(&user_id).unwrap_or(0)
    }
}

impl Drop for RollGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = active_rolls().lock() {
            set.remove(&self.0);
        }
    }
}
