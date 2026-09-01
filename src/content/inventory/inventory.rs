use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::content::Content;

use super::Category;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Inventory {
    pub slots: HashMap<String, Category>,
    pub size: usize,
}

impl Inventory {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            size: 0,
        }
    }
    #[allow(dead_code)]
    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }
    #[allow(dead_code)]
    pub fn with_category(mut self, category: impl Into<String>, size: usize) -> Self {
        self.slots.insert(category.into(), Category::new(size));
        self
    }
    #[allow(dead_code)]
    pub fn add_category(&mut self, category: impl Into<String>, size: usize) -> &mut Category {
        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(size))
    }
    #[allow(dead_code)]
    pub fn has_category(&self, category: &str) -> bool {
        self.slots.contains_key(category)
    }
    #[allow(dead_code)]
    pub fn push(&mut self, category: impl Into<String>, item: Slot) -> Result<(), Slot> {
        let expand_step = if self.size == 0 { 1 } else { self.size };

        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(self.size))
            .push_with_auto_expand(item, expand_step)
    }
    #[allow(dead_code)]
    pub fn set(&mut self, category: impl Into<String>, index: usize, item: Slot) {
        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(self.size))
            .set_or_expand(index, item);
    }
    #[allow(dead_code)]
    pub fn get(&self, category: &str, index: usize) -> Option<Slot> {
        self.slots.get(category).and_then(|slots| slots.get(index))
    }
    #[allow(dead_code)]
    pub fn take(&mut self, category: &str, index: usize) -> Option<Slot> {
        self.slots
            .get_mut(category)
            .and_then(|slots| slots.take(index))
    }
    #[allow(dead_code)]
    pub fn expand_category(&mut self, category: impl Into<String>, additional_slots: usize) {
        self.slots
            .entry(category.into())
            .or_insert_with(|| Category::new(self.size))
            .expand(additional_slots);
    }
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub cat: Content,
}
