use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::content::Content;
use crate::content::types::Cat;

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
    pub fn category(&self, category: &str) -> Option<&Category> {
        self.slots.get(category)
    }

    #[allow(dead_code)]
    pub fn category_mut(&mut self, category: &str) -> Option<&mut Category> {
        self.slots.get_mut(category)
    }

    #[allow(dead_code)]
    pub fn remove_category(&mut self, category: &str) -> Option<Category> {
        self.slots.remove(category)
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
    pub fn get_ref(&self, category: &str, index: usize) -> Option<&Slot> {
        self.slots
            .get(category)
            .and_then(|slots| slots.get_ref(index))
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, category: &str, index: usize) -> Option<&mut Slot> {
        self.slots
            .get_mut(category)
            .and_then(|slots| slots.get_mut(index))
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

    // --- Bulk Queries & Category Iterators ---

    #[allow(dead_code)]
    pub fn categories(&self) -> impl Iterator<Item = (&String, &Category)> {
        self.slots.iter()
    }

    #[allow(dead_code)]
    pub fn categories_mut(&mut self) -> impl Iterator<Item = (&String, &mut Category)> {
        self.slots.iter_mut()
    }

    #[allow(dead_code)]
    pub fn category_names(&self) -> impl Iterator<Item = &str> {
        self.slots.keys().map(|k| k.as_str())
    }

    /// Returns an iterator over all non-empty items in a specific category.
    #[allow(dead_code)]
    pub fn get_all(&self, category: &str) -> Option<impl Iterator<Item = &Slot>> {
        self.slots.get(category).map(|cat| cat.items())
    }

    /// Returns a cloned Vec of all non-empty items in a category.
    #[allow(dead_code)]
    pub fn get_all_cloned(&self, category: &str) -> Option<Vec<Slot>> {
        self.slots.get(category).map(|cat| cat.get_all_items())
    }

    /// Iterates over all occupied items across all categories as `(category_name, slot_index, &Slot)`.
    #[allow(dead_code)]
    pub fn all_items(&self) -> impl Iterator<Item = (&str, usize, &Slot)> {
        self.slots.iter().flat_map(|(cat_name, cat)| {
            cat.indexed_items()
                .map(move |(idx, slot)| (cat_name.as_str(), idx, slot))
        })
    }

    /// Iterates over mutable references to all occupied items across all categories.
    #[allow(dead_code)]
    pub fn all_items_mut(&mut self) -> impl Iterator<Item = (&str, usize, &mut Slot)> {
        self.slots.iter_mut().flat_map(|(cat_name, cat)| {
            let cat_name_str = cat_name.as_str();
            cat.0
                .iter_mut()
                .enumerate()
                .filter_map(move |(idx, opt)| opt.as_mut().map(|slot| (cat_name_str, idx, slot)))
        })
    }

    /// Total count of categories.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Total count of occupied items across all categories.
    #[allow(dead_code)]
    pub fn total_item_count(&self) -> usize {
        self.slots.values().map(|c| c.item_count()).sum()
    }

    /// Total capacity (slots allocated) across all categories.
    #[allow(dead_code)]
    pub fn total_slot_count(&self) -> usize {
        self.slots.values().map(|c| c.len()).sum()
    }

    /// Clears items in a category without removing the category itself.
    #[allow(dead_code)]
    pub fn clear_category(&mut self, category: &str) -> bool {
        if let Some(cat) = self.slots.get_mut(category) {
            cat.clear();
            true
        } else {
            false
        }
    }

    /// Clears all items across all categories.
    #[allow(dead_code)]
    pub fn clear_all(&mut self) {
        for cat in self.slots.values_mut() {
            cat.clear();
        }
    }
}

// --- Trait Implementations ---

impl IntoIterator for Inventory {
    type Item = (String, Category);
    type IntoIter = std::collections::hash_map::IntoIter<String, Category>;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.into_iter()
    }
}

impl<'a> IntoIterator for &'a Inventory {
    type Item = (&'a String, &'a Category);
    type IntoIter = std::collections::hash_map::Iter<'a, String, Category>;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.iter()
    }
}

impl<'a> IntoIterator for &'a mut Inventory {
    type Item = (&'a String, &'a mut Category);
    type IntoIter = std::collections::hash_map::IterMut<'a, String, Category>;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.iter_mut()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub cat: Cat,
}

impl Slot {
    #[allow(dead_code)]
    pub fn new(cat: Cat) -> Self {
        Self { cat }
    }
}
