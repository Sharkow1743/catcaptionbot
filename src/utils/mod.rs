pub mod roll_guard;
pub mod short_id;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

pub fn unix_timestamp() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CappedBuffer<T> {
    capacity: usize,
    data: VecDeque<T>,
}

impl<T> CappedBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            data: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, item: T) {
        let max_cap = if self.capacity == 0 {
            20
        } else {
            self.capacity
        };
        if self.data.len() >= max_cap {
            self.data.pop_front();
        }
        self.data.push_back(item);
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.data.iter()
    }

    pub fn to_vec(self) -> Vec<T> {
        self.into()
    }

    pub fn get_latest(&self) -> Option<&T> {
        self.data.back()
    }

    pub fn get_latest_mut(&mut self) -> Option<&mut T> {
        self.data.back_mut()
    }

    pub fn get_oldest(&self) -> Option<&T> {
        self.data.front()
    }

    pub fn get_oldest_mut(&mut self) -> Option<&mut T> {
        self.data.front_mut()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }
}

impl<T> From<CappedBuffer<T>> for Vec<T> {
    fn from(value: CappedBuffer<T>) -> Self {
        value.data.into()
    }
}

pub fn float_to_progress(val: f32) -> char {
    let scale = ['⠀', '⡀', '⣀', '⣄', '⣤', '⣦', '⣶', '⣷', '⣿'];

    if val <= 0.0 {
        return scale[0];
    }

    if val >= 1.0 {
        return scale[scale.len() - 1];
    }

    // Map the range (0.0, 1.0) across the 10 array indices
    let index = (val * scale.len() as f32).floor() as usize;
    scale[index.min(scale.len() - 1)]
}

pub fn render_progress_bar(ratio: f32, length: usize) -> String {
    let clamped = ratio.clamp(0.0, 1.0);
    let filled = (clamped * length as f32).round() as usize;
    let empty = length.saturating_sub(filled);
    format!("[{}{}]", "■".repeat(filled), "□".repeat(empty))
}
