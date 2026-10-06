//! The category mask that a modifier container's constructor gives it.
//!
//! A category constructor stores its `w2` argument as the container's mask; a default constructor
//! stores the mask of every category. A walk that reaches the construction labels the path with
//! that mask. A member walk also takes the mask word that a modifier destination holds at the end
//! of a path, which an owner that builds the container inline stores itself. A container's mask is
//! established only when every path of every run gives it one known mask; an unknown argument, a
//! path that gives none, and paths that disagree each leave it unresolved, never every category.
//! The rule assumes that no code writes the mask between construction and the engine's check.
use super::records::ModifierContainers;
use crate::engine::analysis::{evaluate::Machine, modifiers::EVERY_CATEGORY, stop::Unresolved};
use std::collections::BTreeSet;

/// The label of a construction's mask on a path. A member walk adds the owner offset.
pub(super) const LABEL: u64 = 1 << 59;

/// A construction whose category argument is not known on its path.
const UNKNOWN: u64 = 1 << 32;

/// A path that does not construct the container.
const MISSING: u64 = 1 << 33;

impl ModifierContainers {
    /// The mask that a call of `target` gives its receiver, when `target` constructs a modifier
    /// container.
    pub(super) fn mask(&self, target: u64, machine: &Machine<'_>) -> Option<u64> {
        if self.default.contains(&target) {
            return Some(EVERY_CATEGORY);
        }

        self.category.contains(&target).then(|| {
            machine
                .register(2)
                .map_or(UNKNOWN, |mask| mask & EVERY_CATEGORY)
        })
    }
}

/// The masks that the paths of the walks give one container.
#[derive(Debug, Default)]
pub(super) struct PathMasks(BTreeSet<u64>);

impl PathMasks {
    /// Add each mask that one path gives the container. A path that gives none does not
    /// construct it.
    pub fn add(&mut self, masks: impl IntoIterator<Item = u64>) {
        let mut masks = masks.into_iter().peekable();

        if masks.peek().is_none() {
            self.0.insert(MISSING);
        }
        self.0.extend(masks);
    }

    /// The one known mask of every path.
    pub fn agreed(&self) -> Result<u64, Unresolved> {
        let mut masks = self.0.iter();

        match (masks.next(), masks.next()) {
            (Some(&mask), None) if mask <= EVERY_CATEGORY => Ok(mask),
            _ if self.0.contains(&UNKNOWN) => Err(Unresolved::new("category-argument")),
            _ if self.0.contains(&MISSING) || self.0.is_empty() => {
                Err(Unresolved::new("container-unreached"))
            }
            _ => Err(Unresolved::new("container-paths-disagree")),
        }
    }
}
