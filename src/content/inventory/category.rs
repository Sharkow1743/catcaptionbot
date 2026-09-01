use serde::{Deserialize, Serialize};

use super::inventory::Slot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category(Box<[Option<Slot>]>);

impl Category {
    #[allow(dead_code)]
    pub fn new(size: usize) -> Self {
        Self(vec![None; size].into_boxed_slice())
    }
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    #[allow(dead_code)]
    pub fn get(&self, index: usize) -> Option<Slot> {
        self.0.get(index).cloned().flatten()
    }
    #[allow(dead_code)]
    pub fn get_ref(&self, index: usize) -> Option<&Slot> {
        self.0.get(index).and_then(|opt| opt.as_ref())
    }
    #[allow(dead_code)]
    pub fn set_or_expand(&mut self, index: usize, item: Slot) {
        if index >= self.0.len() {
            let additional_slots = index + 1 - self.0.len();
            self.expand(additional_slots);
        }
        self.0[index] = Some(item);
    }
    #[allow(dead_code)]
    pub fn take(&mut self, index: usize) -> Option<Slot> {
        self.0.get_mut(index).and_then(|slot| slot.take())
    }
    #[allow(dead_code)]
    pub fn push_with_auto_expand(&mut self, item: Slot, expand_by: usize) -> Result<(), Slot> {
        if self.0.is_empty() {
            self.expand(expand_by.max(1));
        }

        if let Some(empty_slot) = self.0.iter_mut().find(|s| s.is_none()) {
            *empty_slot = Some(item);
            return Ok(());
        }

        Err(item)
    }
    #[allow(dead_code)]
    pub fn expand(&mut self, additional_slots: usize) {
        let mut new_vec = self.0.to_vec();
        new_vec.resize(new_vec.len() + additional_slots, None);
        self.0 = new_vec.into_boxed_slice();
    }
}
