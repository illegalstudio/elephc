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

use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
mod tests;

pub(crate) const HEADER_SIZE: usize = 64;
pub(crate) const ENTRIES_OFFSET: usize = 40;
pub(crate) const PINS_OFFSET: usize = 48;
/// Signed next automatic integer index; i64::MIN marks an array with no integer insertions.
pub(crate) const NEXT_INDEX_OFFSET: usize = 56;
pub(crate) const ENTRY_SIZE: usize = 64;

/// Loads the owned entry-storage pointer without changing the hash header register.
pub(crate) fn emit_entries(emitter: &mut Emitter, destination: &str, hash: &str) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {destination}, QWORD PTR [{hash} + {ENTRIES_OFFSET}]")); // borrow the separate entry allocation
    } else {
        emitter.instruction(&format!("ldr {destination}, [{hash}, #{ENTRIES_OFFSET}]")); // borrow the separate entry allocation
    }
}

/// Computes an entry address while preserving the header and logical index registers.
pub(crate) fn emit_entry_address(emitter: &mut Emitter, destination: &str, hash: &str, index: &str) {
    assert_ne!(destination, hash, "hash address calculation must preserve its header");
    assert_ne!(destination, index, "hash address calculation must preserve its index");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {destination}, {index}"));            // copy the logical slot index
        emitter.instruction(&format!("shl {destination}, {}", ENTRY_SIZE.trailing_zeros())); // scale to an entry byte offset
        emitter.instruction(&format!("add {destination}, QWORD PTR [{hash} + {ENTRIES_OFFSET}]")); // address the slot in separate storage
    } else {
        emit_entries(emitter, destination, hash);
        emitter.instruction(&format!("add {destination}, {destination}, {index}, lsl #{}", ENTRY_SIZE.trailing_zeros())); // address the indexed entry
    }
}
