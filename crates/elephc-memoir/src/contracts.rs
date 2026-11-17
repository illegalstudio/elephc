//! Purpose:
//! Defines disjoint tag domains and validated ownership-layout facts.
//!
//! Called from:
//! - Compiler adapters and memory emitters at their typed configuration boundary.
//!
//! Key details:
//! - These values configure assembly emission inside the compiler.
//! - Heap kinds, value tags, record offsets and frame offsets are distinct Rust types.

use core::fmt;

/// An eight-bit heap storage kind, without collector flags or value-type lanes.
///
/// Heap storage kinds cannot be passed as runtime value tags.
/// ```compile_fail
/// use elephc_memoir::{HeapKind, ValueTag};
/// let value_tag: ValueTag = HeapKind::new(5);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct HeapKind(u32);

impl HeapKind {
    /// Rejects packed header metadata where only a storage-kind ID is valid.
    pub const fn new(id: u32) -> Self {
        assert!(id <= 0xff, "heap kind must contain only the low kind byte");
        Self(id)
    }
    /// Returns the storage-kind ID for an authoritative target encoding.
    pub const fn raw(self) -> u32 { self.0 }
}

impl fmt::Display for HeapKind {
    /// Keeps emitted decimal kind operands identical to the existing assembly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(&self.0, f) }
}

/// A boxed/container runtime value tag selected by compiler semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ValueTag(u8);

impl ValueTag {
    /// Wraps a compiler-selected ABI tag without importing PHP type definitions.
    pub const fn new(tag: u8) -> Self { Self(tag) }
    /// Returns the runtime ABI tag for an authoritative target encoding.
    pub const fn raw(self) -> u8 { self.0 }
}

impl fmt::Display for ValueTag {
    /// Keeps emitted decimal tag operands identical to the existing assembly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(&self.0, f) }
}

/// An aligned byte offset of pointer-sized storage inside an ownership record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct RecordOffset(usize);

impl RecordOffset {
    /// Rejects offsets that cannot address a complete aligned owner word.
    pub const fn new(bytes: usize) -> Self {
        assert!(bytes % 8 == 0, "record owner offset must be word-aligned");
        Self(bytes)
    }
    /// Returns bytes relative to the record base.
    pub const fn bytes(self) -> usize { self.0 }
}

/// An aligned byte offset of a pointer-sized spill relative to the active frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct FrameOffset(usize);

impl FrameOffset {
    /// Rejects offsets that cannot address an aligned callback argument spill.
    pub const fn new(bytes: usize) -> Self {
        assert!(bytes % 8 == 0, "frame argument offset must be word-aligned");
        Self(bytes)
    }
    /// Returns bytes relative to the active frame base.
    pub const fn bytes(self) -> usize { self.0 }
}

/// Owner-record geometry paired with its protected callback argument location.
///
/// The two bases cannot be swapped accidentally.
/// ```compile_fail
/// use elephc_memoir::{FrameOffset, GuardLayout, RecordOffset};
/// let layout = GuardLayout::new(FrameOffset::new(224), RecordOffset::new(24));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardLayout {
    owner: RecordOffset,
    argument: FrameOffset,
}

impl GuardLayout {
    /// Keeps the owner beyond the standard previous/callback/data activation prefix.
    pub const fn new(owner: RecordOffset, argument: FrameOffset) -> Self {
        assert!(owner.bytes() >= 24, "guard owner must follow the activation prefix");
        Self { owner, argument }
    }
    /// Returns the owner location relative to its guard record.
    pub const fn owner(self) -> RecordOffset { self.owner }
    /// Returns the guard argument spill relative to the protected callback frame.
    pub const fn argument(self) -> FrameOffset { self.argument }
}

/// Explicit state symbols for a depth-stamped temporary-owner list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopedOwnerState<'a> {
    /// Mutable head pointer that owns the current list prefix.
    pub head: &'a str,
    /// Caller-owned active scope depth used to delimit that prefix.
    pub depth: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packed collector/value metadata must never become a storage-kind predicate.
    #[test]
    #[should_panic(expected = "heap kind must contain only the low kind byte")]
    fn heap_kind_rejects_packed_metadata() { let _ = HeapKind::new(0x105); }

    /// Misaligned owner fields would clear neighboring record state.
    #[test]
    #[should_panic(expected = "record owner offset must be word-aligned")]
    fn record_offset_rejects_partial_words() { let _ = RecordOffset::new(25); }

    /// Misaligned callback spills would reload a partial native pointer.
    #[test]
    #[should_panic(expected = "frame argument offset must be word-aligned")]
    fn frame_offset_rejects_partial_words() { let _ = FrameOffset::new(225); }

    /// An owner field overlapping the activation prefix would corrupt unwinding.
    #[test]
    #[should_panic(expected = "guard owner must follow the activation prefix")]
    fn guard_layout_rejects_activation_prefix_overlap() {
        let _ = GuardLayout::new(RecordOffset::new(16), FrameOffset::new(224));
    }
}
