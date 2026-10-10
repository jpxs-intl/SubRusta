use std::{cmp::Reverse, collections::BinaryHeap};

/// Slots handed out lowest free index first, like the game's fixed arrays, up to `capacity` of them.
pub struct Table<T> {
    slots: Vec<Option<T>>,
    free: BinaryHeap<Reverse<usize>>,
    capacity: usize,
}

impl<T> Table<T> {
    pub fn new(capacity: usize) -> Self {
        Self { slots: Vec::new(), free: BinaryHeap::new(), capacity }
    }

    /// A table that grows as far as it is filled.
    pub fn unbounded() -> Self {
        Self::new(usize::MAX)
    }

    pub fn insert(&mut self, value: T) -> Option<usize> {
        if let Some(Reverse(i)) = self.free.pop() {
            self.slots[i] = Some(value);
            return Some(i);
        }
        if self.slots.len() >= self.capacity {
            return None;
        }
        self.slots.push(Some(value));
        Some(self.slots.len() - 1)
    }

    pub fn vacant(&self) -> Option<usize> {
        match self.free.peek() {
            Some(&Reverse(i)) => Some(i),
            None if self.slots.len() < self.capacity => Some(self.slots.len()),
            None => None,
        }
    }

    pub fn remove(&mut self, i: usize) -> Option<T> {
        let value = self.slots.get_mut(i)?.take()?;
        self.free.push(Reverse(i));
        Some(value)
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.slots.get(i)?.as_ref()
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.slots.get_mut(i)?.as_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| Some((i, s.as_ref()?)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
        self.slots.iter_mut().enumerate().filter_map(|(i, s)| Some((i, s.as_mut()?)))
    }

    pub fn ids(&self) -> Vec<usize> {
        self.iter().map(|(i, _)| i).collect()
    }
}
