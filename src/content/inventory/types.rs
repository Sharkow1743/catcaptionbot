use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::content::Content;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Inventory {
    pub slots: HashMap<String, Category>,
    pub size: usize,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            size: 0,
        }
    }

    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    pub fn with_category(mut self, category: impl Into<String>, size: usize) -> Self {
        self.slots.insert(category.into(), Category::new(size));
        self
    }

    pub fn add_category(&mut self, category: impl Into<String>, size: usize) -> &mut Category {
        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(size))
    }

    pub fn has_category(&self, category: &str) -> bool {
        self.slots.contains_key(category)
    }

    pub fn push(&mut self, category: impl Into<String>, item: Slot) -> Result<(), Slot> {
        let expand_step = if self.size == 0 { 1 } else { self.size };

        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(self.size))
            .push_with_auto_expand(item, expand_step)
    }

    pub fn set(&mut self, category: impl Into<String>, index: usize, item: Slot) {
        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(self.size))
            .set_or_expand(index, item);
    }

    pub fn get(&self, category: &str, index: usize) -> Option<Slot> {
        self.slots.get(category).and_then(|slots| slots.get(index))
    }

    pub fn take(&mut self, category: &str, index: usize) -> Option<Slot> {
        self.slots
            .get_mut(category)
            .and_then(|slots| slots.take(index))
    }

    pub fn expand_category(&mut self, category: impl Into<String>, additional_slots: usize) {
        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(self.size))
            .expand(additional_slots);
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category(Box<[Option<Slot>]>);

impl Category {
    pub fn new(size: usize) -> Self {
        Self(vec![None; size].into_boxed_slice())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<Slot> {
        self.0.get(index).cloned().flatten()
    }

    pub fn get_ref(&self, index: usize) -> Option<&Slot> {
        self.0.get(index).and_then(|opt| opt.as_ref())
    }

    pub fn set_or_expand(&mut self, index: usize, item: Slot) {
        if index >= self.0.len() {
            let additional_slots = index + 1 - self.0.len();
            self.expand(additional_slots);
        }
        self.0[index] = Some(item);
    }

    pub fn take(&mut self, index: usize) -> Option<Slot> {
        self.0.get_mut(index).and_then(|slot| slot.take())
    }

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

    pub fn expand(&mut self, additional_slots: usize) {
        let mut new_vec = self.0.to_vec();
        new_vec.resize(new_vec.len() + additional_slots, None);
        self.0 = new_vec.into_boxed_slice();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub cat: Content,
}
