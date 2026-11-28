//! Purpose:
//! Owns the native directory envelope around libc glob state and its iteration prefix.
//!
//! Called from:
//! - Directory-glob open, iteration, rewind and close emitters.
//!
//! Key details:
//! - Fresh glob_t storage is zeroed so a failed glob can release partial results.
//! - libc releases the result vector before the runtime heap reclaims the envelope.

use crate::{Arch, Emitter};

const ENVELOPE_BYTES: usize = 160;
const PATHS_OFFSET: usize = 0;
const COUNT_OFFSET: usize = 8;
const INDEX_OFFSET: usize = 16;
const LIBC_STATE_OFFSET: usize = 24;

/// Allocates an envelope and zeroes its embedded glob_t before passing it to libc.
pub fn emit_allocate(emitter: &mut impl Emitter) {
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, ENVELOPE_BYTES as i64);
    emitter.emit_call_label("__rt_heap_alloc");
    for offset in (LIBC_STATE_OFFSET..ENVELOPE_BYTES).step_by(8) {
        emitter.emit_store_zero_to_address(result, offset);
    }
}

/// Materializes the address of the libc-managed portion of an envelope.
pub fn emit_glob_address(emitter: &mut impl Emitter, destination: &str, envelope: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {destination}, {envelope}, #{LIBC_STATE_OFFSET}")); // address the embedded glob_t
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {destination}, [{envelope} + {LIBC_STATE_OFFSET}]")); // address the embedded glob_t
        }
    }
}

/// Initializes the iterator prefix after libc has filled its own structure.
pub fn emit_initialize(emitter: &mut impl Emitter, slot: usize, pathv_offset: usize) {
    let (envelope, value) = match emitter.arch() {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r9", "r10"),
    };
    emit_load_envelope(emitter, envelope, slot);
    emitter.emit_load_from_address(value, envelope, LIBC_STATE_OFFSET);
    emitter.emit_store_to_address(value, envelope, COUNT_OFFSET);
    emitter.emit_load_from_address(value, envelope, LIBC_STATE_OFFSET + pathv_offset);
    emitter.emit_store_to_address(value, envelope, PATHS_OFFSET);
    emitter.emit_store_zero_to_address(envelope, INDEX_OFFSET);
}

/// Loads and advances a borrowed path from the envelope in x10 or r10.
/// The caller owns the exhaustion branch and converts the borrowed C string.
pub fn emit_next_path(emitter: &mut impl Emitter, exhausted: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_load_from_address("x11", "x10", COUNT_OFFSET);
            emitter.emit_load_from_address("x12", "x10", INDEX_OFFSET);
            emitter.instruction("cmp x12, x11");                                // compare the cursor with the match count
            emitter.instruction(&format!("b.hs {exhausted}"));                  // leave through the caller's end-of-directory path
            emitter.emit_load_from_address("x13", "x10", PATHS_OFFSET);
            emitter.instruction("ldr x1, [x13, x12, lsl #3]");                  // borrow the next libc path
            emitter.instruction("add x12, x12, #1");                            // advance the entry index
            emitter.emit_store_to_address("x12", "x10", INDEX_OFFSET);
        }
        Arch::X86_64 => {
            emitter.emit_load_from_address("r11", "r10", COUNT_OFFSET);
            emitter.emit_load_from_address("rdx", "r10", INDEX_OFFSET);
            emitter.instruction("cmp rdx, r11");                                // compare the cursor with the match count
            emitter.instruction(&format!("jae {exhausted}"));                   // leave through the caller's end-of-directory path
            emitter.emit_load_from_address("r8", "r10", PATHS_OFFSET);
            emitter.instruction("mov rsi, QWORD PTR [r8 + rdx * 8]");           // borrow the next libc path
            emitter.instruction("add rdx, 1");                                  // advance the entry index
            emitter.emit_store_to_address("rdx", "r10", INDEX_OFFSET);
        }
    }
}

/// Resets the iterator without touching the libc result.
pub fn emit_rewind(emitter: &mut impl Emitter, envelope: &str) {
    emitter.emit_store_zero_to_address(envelope, INDEX_OFFSET);
}

/// Releases partial or complete libc results, then reclaims the saved envelope.
pub fn emit_release(emitter: &mut impl Emitter, slot: usize) {
    let argument = emitter.int_arg_reg_name(0);
    emit_load_envelope(emitter, argument, slot);
    emit_glob_address(emitter, argument, argument);
    emitter.bl_c("globfree");
    let result = emitter.int_result_reg();
    emit_load_envelope(emitter, result, slot);
    emitter.emit_call_label("__rt_heap_free");
}

/// Reloads a pointer from the caller's frame using its native spill convention.
fn emit_load_envelope(emitter: &mut impl Emitter, reg: &str, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {reg}, [sp, #{slot}]"));          // reload the saved envelope
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {reg}, QWORD PTR [rbp - {slot}]")); // reload the saved envelope
        }
    }
}
