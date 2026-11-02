//! Purpose:
//! Stores native metadata locally during registration and shares it after publication.
//!
//! Called from:
//! - `super` and native metadata accessors on `ElephcEvalContext`.
//!
//! Key details:
//! - Empty activation state does not allocate an Arc.
//! - Shared handles are `Arc` because the process-global AOT snapshot crosses threads.
//! - Explicit metadata writes detach with copy-on-write; ordinary lookup never clones it.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Native metadata that is mutable until frozen or explicitly detached by its context.
#[derive(Clone)]
pub(crate) enum SharedNative<T> {
    Local(T),
    Shared(Arc<T>),
}

impl<T: Default> Default for SharedNative<T> {
    /// Creates empty local storage without a reference-count allocation.
    fn default() -> Self { Self::Local(T::default()) }
}

impl<T> SharedNative<T> {
    /// Attaches one already frozen metadata allocation.
    pub(crate) fn from_shared(value: Arc<T>) -> Self { Self::Shared(value) }

    /// Reports whether this handle is the very allocation `other` points at.
    pub(crate) fn shares_with(&self, other: &Arc<T>) -> bool {
        matches!(self, Self::Shared(value) if Arc::ptr_eq(value, other))
    }
}

impl<T: Default> SharedNative<T> {
    /// Freezes local metadata by moving it, without cloning its entries.
    pub(crate) fn share(&mut self) -> Arc<T> {
        let value = match std::mem::take(self) {
            Self::Local(value) => Arc::new(value),
            Self::Shared(value) => value,
        };
        *self = Self::Shared(Arc::clone(&value));
        value
    }
}

impl<T> Deref for SharedNative<T> {
    type Target = T;

    /// Borrows native metadata without materialization.
    fn deref(&self) -> &T {
        match self { Self::Local(value) => value, Self::Shared(value) => value }
    }
}

impl<T: Clone> DerefMut for SharedNative<T> {
    /// Gives an explicit writer private storage while preserving the cached catalog.
    fn deref_mut(&mut self) -> &mut T {
        match self { Self::Local(value) => value, Self::Shared(value) => Arc::make_mut(value) }
    }
}

impl<'a, T> IntoIterator for &'a SharedNative<T> where &'a T: IntoIterator {
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;

    /// Iterates the borrowed collection without copying entries.
    fn into_iter(self) -> Self::IntoIter { self.deref().into_iter() }
}

impl<'a, T: Clone> IntoIterator for &'a mut SharedNative<T> where &'a mut T: IntoIterator {
    type Item = <&'a mut T as IntoIterator>::Item;
    type IntoIter = <&'a mut T as IntoIterator>::IntoIter;

    /// Detaches before exposing mutable elements.
    fn into_iter(self) -> Self::IntoIter { self.deref_mut().into_iter() }
}
