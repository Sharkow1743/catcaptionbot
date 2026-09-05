// src/content/inventory/category.rs

use serde::{Deserialize, Serialize};

use super::inventory::Slot;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Category(pub Vec<Option<Slot>>);

impl Category {
    #[allow(dead_code)]
    pub fn new(size: usize) -> Self {
        Self(vec![None; size])
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
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Slot> {
        self.0.get_mut(index).and_then(|opt| opt.as_mut())
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
        self.0.extend(vec![None; additional_slots]);
    }

    // --- Slices & Iterators ---

    #[allow(dead_code)]
    pub fn as_slice(&self) -> &[Option<Slot>] {
        &self.0
    }

    #[allow(dead_code)]
    pub fn as_mut_slice(&mut self) -> &mut [Option<Slot>] {
        &mut self.0
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> std::slice::Iter<'_, Option<Slot>> {
        self.0.iter()
    }

    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Option<Slot>> {
        self.0.iter_mut()
    }

    /// Iterator over references to occupied items only (filters out `None`).
    #[allow(dead_code)]
    pub fn items(&self) -> impl Iterator<Item = &Slot> {
        self.0.iter().filter_map(|opt| opt.as_ref())
    }

    /// Iterator over mutable references to occupied items only (filters out `None`).
    #[allow(dead_code)]
    pub fn items_mut(&mut self) -> impl Iterator<Item = &mut Slot> {
        self.0.iter_mut().filter_map(|opt| opt.as_mut())
    }

    /// Iterator over `(index, &Slot)` for all occupied slots.
    #[allow(dead_code)]
    pub fn indexed_items(&self) -> impl Iterator<Item = (usize, &Slot)> {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(idx, opt)| opt.as_ref().map(|slot| (idx, slot)))
    }

    /// Returns a list of all non-empty items cloned.
    #[allow(dead_code)]
    pub fn get_all_items(&self) -> Vec<Slot> {
        self.items().cloned().collect()
    }

    // --- Capacity & Query Helpers ---

    /// Number of occupied slots.
    #[allow(dead_code)]
    pub fn item_count(&self) -> usize {
        self.0.iter().filter(|s| s.is_some()).count()
    }

    /// Number of empty (`None`) slots.
    #[allow(dead_code)]
    pub fn free_slot_count(&self) -> usize {
        self.0.iter().filter(|s| s.is_none()).count()
    }

    #[allow(dead_code)]
    pub fn has_free_slots(&self) -> bool {
        self.0.iter().any(|s| s.is_none())
    }

    #[allow(dead_code)]
    pub fn first_empty_index(&self) -> Option<usize> {
        self.0.iter().position(|s| s.is_none())
    }

    /// Clears all slots (sets them to `None`) without changing the capacity.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        for slot in &mut self.0 {
            *slot = None;
        }
    }
}

// --- Trait Implementations ---

impl IntoIterator for Category {
    type Item = Option<Slot>;
    type IntoIter = std::vec::IntoIter<Option<Slot>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Category {
    type Item = &'a Option<Slot>;
    type IntoIter = std::slice::Iter<'a, Option<Slot>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Category {
    type Item = &'a mut Option<Slot>;
    type IntoIter = std::slice::IterMut<'a, Option<Slot>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl From<Vec<Option<Slot>>> for Category {
    fn from(vec: Vec<Option<Slot>>) -> Self {
        Self(vec)
    }
}

impl FromIterator<Option<Slot>> for Category {
    fn from_iter<I: IntoIterator<Item = Option<Slot>>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl FromIterator<Slot> for Category {
    fn from_iter<I: IntoIterator<Item = Slot>>(iter: I) -> Self {
        Self(iter.into_iter().map(Some).collect())
    }
}
