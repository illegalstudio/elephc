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
/// Link to the next occupied slot in an entry's insertion-order chain.
pub const ENTRY_NEXT_OFFSET: usize = 56;

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

/// Reads one insertion-order endpoint; the caller chooses the PHP operation and empty result.
pub fn emit_order_edge(emitter: &mut impl Emitter, destination: &str, hash: &str, tail: bool) {
    let offset = if tail { ORDER_TAIL_OFFSET } else { ORDER_HEAD_OFFSET };
    emitter.emit_load_from_address(destination, hash, offset);
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
        emitter.instruction(&format!("ldp {low}, {high}, [{entry}, #8]"));       // borrow the key's paired words
    } else {
        emitter.emit_load_from_address(low, entry, 8);
        emitter.emit_load_from_address(high, entry, 16);
    }
}

/// Loads the tag and both payload words without acquiring the entry's value.
pub fn emit_entry_value(emitter: &mut impl Emitter, entry: &str, tag: &str, low: &str, high: &str) {
    emitter.emit_load_from_address(tag, entry, 40);
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("ldp {low}, {high}, [{entry}, #24]"));      // borrow the value's paired words
    } else {
        emitter.emit_load_from_address(low, entry, 24);
        emitter.emit_load_from_address(high, entry, 32);
    }
}
