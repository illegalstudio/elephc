//! Purpose:
//! Defines stable associative-array headers and addresses their separate entry storage.
//!
//! Called from:
//! - Hash allocation, mutation, iteration, GC, and EIR hash lowering.
//!
//! Key details:
//! - Rehashing replaces entry storage without relocating the owning header.
//! - Entry addresses remain borrowed and must be reacquired after a mutation.
//! - Lifetime pins keep storage alive without becoming PHP copy-on-write owners.

use crate::{Emitter, Arch};

/// Live-entry count in the stable header.
pub use crate::values::container_header::COUNT_OFFSET;
/// Entry-slot capacity in the stable header.
pub const CAPACITY_OFFSET: usize = 8;
/// Uniform value tag, or the heterogeneous marker, in the stable header.
pub const VALUE_TYPE_OFFSET: usize = 16;
pub const HEADER_SIZE: usize = 64;
/// First occupied slot in insertion order.
pub const ORDER_HEAD_OFFSET: usize = 24;
/// Last occupied slot in insertion order.
pub const ORDER_TAIL_OFFSET: usize = 32;
pub const ENTRIES_OFFSET: usize = 40;
pub const PINS_OFFSET: usize = 48;
/// Signed next automatic integer index; i64::MIN marks an array with no integer insertions.
pub const NEXT_INDEX_OFFSET: usize = 56;
pub const ENTRY_SIZE: usize = 64;
/// Occupancy marker in one entry.
pub const ENTRY_OCCUPIED_OFFSET: usize = 0;
/// Low key word in one entry.
pub const ENTRY_KEY_LOW_OFFSET: usize = 8;
/// String length or integer sentinel in one entry.
pub const ENTRY_KEY_HIGH_OFFSET: usize = 16;
/// Low value word in one entry.
pub const ENTRY_VALUE_LOW_OFFSET: usize = 24;
/// High value word in one entry.
pub const ENTRY_VALUE_HIGH_OFFSET: usize = 32;
/// Runtime value tag in one entry.
pub const ENTRY_VALUE_TAG_OFFSET: usize = 40;
/// Link to the previous occupied slot in insertion order.
pub const ENTRY_PREVIOUS_OFFSET: usize = 48;
/// Link to the next occupied slot in an entry's insertion-order chain.
pub const ENTRY_NEXT_OFFSET: usize = 56;

/// Selects one physical word from the stable hash header.
#[derive(Clone, Copy)]
pub enum HeaderField {
    /// Live-entry count.
    Count,
    /// Entry-slot capacity.
    Capacity,
    /// Uniform or heterogeneous value type.
    ValueType,
    /// First occupied insertion-order slot.
    Head,
    /// Last occupied insertion-order slot.
    Tail,
    /// Separate entry-storage pointer.
    Entries,
    /// Non-owner lifetime pin count.
    Pins,
    /// Next automatic integer key.
    NextIndex,
}

impl HeaderField {
    /// Returns the selected header word's byte offset.
    const fn offset(self) -> usize {
        match self {
            Self::Count => COUNT_OFFSET,
            Self::Capacity => CAPACITY_OFFSET,
            Self::ValueType => VALUE_TYPE_OFFSET,
            Self::Head => ORDER_HEAD_OFFSET,
            Self::Tail => ORDER_TAIL_OFFSET,
            Self::Entries => ENTRIES_OFFSET,
            Self::Pins => PINS_OFFSET,
            Self::NextIndex => NEXT_INDEX_OFFSET,
        }
    }
}

/// Selects one physical word from a hash entry.
#[derive(Clone, Copy)]
pub enum EntryField {
    /// Occupancy marker.
    Occupied,
    /// Key pointer or integer payload.
    KeyLow,
    /// String length or integer sentinel.
    KeyHigh,
    /// Low value payload.
    ValueLow,
    /// High value payload.
    ValueHigh,
    /// Runtime value tag.
    ValueTag,
    /// Previous insertion-order slot.
    Previous,
    /// Next insertion-order slot.
    Next,
}

impl EntryField {
    /// Returns the selected entry word's byte offset.
    const fn offset(self) -> usize {
        match self {
            Self::Occupied => ENTRY_OCCUPIED_OFFSET,
            Self::KeyLow => ENTRY_KEY_LOW_OFFSET,
            Self::KeyHigh => ENTRY_KEY_HIGH_OFFSET,
            Self::ValueLow => ENTRY_VALUE_LOW_OFFSET,
            Self::ValueHigh => ENTRY_VALUE_HIGH_OFFSET,
            Self::ValueTag => ENTRY_VALUE_TAG_OFFSET,
            Self::Previous => ENTRY_PREVIOUS_OFFSET,
            Self::Next => ENTRY_NEXT_OFFSET,
        }
    }
}

/// Loads one stable header word without exposing its byte offset to the caller.
pub fn emit_load_header_field(
    emitter: &mut impl Emitter, destination: &str, hash: &str, field: HeaderField,
) {
    emitter.emit_load_from_address(destination, hash, field.offset());
}

/// Stores one stable header word without exposing its byte offset to the caller.
pub fn emit_store_header_field(
    emitter: &mut impl Emitter, source: &str, hash: &str, field: HeaderField,
) {
    emitter.emit_store_to_address(source, hash, field.offset());
}

/// Loads one borrowed entry word without exposing its byte offset to the caller.
pub fn emit_load_entry_field(
    emitter: &mut impl Emitter, destination: &str, entry: &str, field: EntryField,
) {
    emitter.emit_load_from_address(destination, entry, field.offset());
}

/// Stores one entry word without exposing its byte offset to the caller.
pub fn emit_store_entry_field(
    emitter: &mut impl Emitter, source: &str, entry: &str, field: EntryField,
) {
    emitter.emit_store_to_address(source, entry, field.offset());
}

/// Addresses the low value word inside one borrowed entry.
pub fn emit_value_address_from_entry(
    emitter: &mut impl Emitter, destination: &str, entry: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!(
            "add {destination}, {entry}, #{ENTRY_VALUE_LOW_OFFSET}"
        )),                                                                     // address the borrowed entry value payload
        Arch::X86_64 => emitter.instruction(&format!(
            "lea {destination}, [{entry} + {ENTRY_VALUE_LOW_OFFSET}]"
        )),                                                                     // address the borrowed entry value payload
    }
}

/// Loads the tag adjacent to a caller's borrowed value-low address.
pub fn emit_load_value_tag_from_value_address(
    emitter: &mut impl Emitter, destination: &str, value: &str,
) {
    emitter.emit_load_from_address(
        destination,
        value,
        ENTRY_VALUE_TAG_OFFSET - ENTRY_VALUE_LOW_OFFSET,
    );
}

/// Loads the managed owner stored at a caller's borrowed value-low address.
pub fn emit_load_value_owner_from_value_address(
    emitter: &mut impl Emitter, destination: &str, value: &str,
) {
    emitter.emit_load_from_address(destination, value, 0);
}

/// Loads the entry-storage pointer into the selected register.
pub fn emit_entries(emitter: &mut impl Emitter, destination: &str, hash: &str) {
    if emitter.arch() == Arch::X86_64 {
        emitter.instruction(&format!("mov {destination}, QWORD PTR [{hash} + {ENTRIES_OFFSET}]")); // borrow the separate entry allocation
    } else {
        emitter.instruction(&format!("ldr {destination}, [{hash}, #{ENTRIES_OFFSET}]")); // borrow the separate entry allocation
    }
}

/// Computes an entry address while preserving the header and logical index registers.
pub fn emit_entry_address(emitter: &mut impl Emitter, destination: &str, hash: &str, index: &str) {
    assert_ne!(destination, hash, "hash address calculation must preserve its header");
    assert_ne!(destination, index, "hash address calculation must preserve its index");
    if emitter.arch() == Arch::X86_64 {
        emitter.instruction(&format!("mov {destination}, {index}"));            // copy the logical slot index
        emitter.instruction(&format!("shl {destination}, {}", ENTRY_SIZE.trailing_zeros())); // scale to an entry byte offset
        emitter.instruction(&format!("add {destination}, QWORD PTR [{hash} + {ENTRIES_OFFSET}]")); // address the slot in separate storage
    } else {
        emit_entries(emitter, destination, hash);
        emitter.instruction(&format!("add {destination}, {destination}, {index}, lsl #{}", ENTRY_SIZE.trailing_zeros())); // address the indexed entry
    }
}

/// Computes an entry address from a borrowed entry-storage base and logical slot index.
pub fn emit_entry_address_from_storage(
    emitter: &mut impl Emitter, destination: &str, entries: &str, index: &str,
) {
    assert_ne!(destination, entries, "entry address calculation must preserve its storage base");
    let shift = ENTRY_SIZE.trailing_zeros();
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("add {destination}, {entries}, {index}, lsl #{shift}")), // address the indexed entry in borrowed storage
        Arch::X86_64 => {
            if destination != index {
                emitter.instruction(&format!("mov {destination}, {index}"));    // copy the logical slot index
            }
            emitter.instruction(&format!("shl {destination}, {shift}"));        // scale to the entry byte displacement
            emitter.instruction(&format!("add {destination}, {entries}"));      // address the indexed entry in borrowed storage
        }
    }
}

/// Addresses an entry in a borrowed storage base, consuming the index on x86_64.
pub fn emit_consume_index_entry_address(
    emitter: &mut impl Emitter, entries: &str, index: &str,
) {
    let shift = ENTRY_SIZE.trailing_zeros();
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("add {entries}, {entries}, {index}, lsl #{shift}")), // address the indexed entry in place
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {index}, {shift}"));              // scale the consumed slot to its byte displacement
            emitter.instruction(&format!("add {entries}, {index}"));            // address the indexed entry in place
        }
    }
}

/// Converts one borrowed entry address back to its logical slot index.
pub fn emit_entry_index(
    emitter: &mut impl Emitter, destination: &str, entry: &str, entries: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("sub {destination}, {entry}, {entries}")); // convert the entry address to a byte displacement
            emitter.instruction(&format!("lsr {destination}, {destination}, #{}", ENTRY_SIZE.trailing_zeros())); // convert the byte displacement to a slot index
        }
        Arch::X86_64 => {
            if destination != entry {
                emitter.emit_reg_move(destination, entry);
            }
            emitter.instruction(&format!("sub {destination}, {entries}"));      // convert the entry address to a byte displacement
            emitter.instruction(&format!("shr {destination}, {}", ENTRY_SIZE.trailing_zeros())); // convert the byte displacement to a slot index
        }
    }
}

/// Reads one insertion-order endpoint; the caller chooses the PHP operation and empty result.
pub fn emit_order_edge(emitter: &mut impl Emitter, destination: &str, hash: &str, tail: bool) {
    emit_load_header_field(
        emitter,
        destination,
        hash,
        if tail { HeaderField::Tail } else { HeaderField::Head },
    );
}

/// Addresses an entry by consuming a scratch copy of the slot index and the header register.
pub fn emit_consume_entry_address(emitter: &mut impl Emitter, hash: &str, index: &str) {
    emit_entries(emitter, hash, hash);
    let shift = ENTRY_SIZE.trailing_zeros();
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("add {index}, {hash}, {index}, lsl #{shift}")), // address the borrowed entry
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {index}, {shift}"));              // scale the slot to its byte offset
            emitter.instruction(&format!("add {index}, {hash}"));               // address the borrowed entry
        }
    }
}

/// Loads the key's low word and its length or integer sentinel from a borrowed entry.
pub fn emit_entry_key(emitter: &mut impl Emitter, entry: &str, low: &str, high: &str) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("ldp {low}, {high}, [{entry}, #{ENTRY_KEY_LOW_OFFSET}]")); // borrow the key's paired words
    } else {
        emit_load_entry_field(emitter, low, entry, EntryField::KeyLow);
        emit_load_entry_field(emitter, high, entry, EntryField::KeyHigh);
    }
}

/// Loads the tag and both payload words without acquiring the entry's value.
pub fn emit_entry_value(emitter: &mut impl Emitter, entry: &str, tag: &str, low: &str, high: &str) {
    emit_load_entry_field(emitter, tag, entry, EntryField::ValueTag);
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("ldp {low}, {high}, [{entry}, #{ENTRY_VALUE_LOW_OFFSET}]")); // borrow the value's paired words
    } else {
        emit_load_entry_field(emitter, low, entry, EntryField::ValueLow);
        emit_load_entry_field(emitter, high, entry, EntryField::ValueHigh);
    }
}

/// Stores both payload words and the runtime tag into one borrowed entry.
pub fn emit_store_entry_value(
    emitter: &mut impl Emitter, entry: &str, tag: &str, low: &str, high: &str,
) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!(
            "stp {low}, {high}, [{entry}, #{ENTRY_VALUE_LOW_OFFSET}]"
        ));                                                                     // publish the paired value payload words
    } else {
        emit_store_entry_field(emitter, low, entry, EntryField::ValueLow);
        emit_store_entry_field(emitter, high, entry, EntryField::ValueHigh);
    }
    emit_store_entry_field(emitter, tag, entry, EntryField::ValueTag);
}
