//! Purpose:
//! Defines native iterator frame storage and owns its saved successor keys.
//!
//! Called from:
//! - EIR iterator lowering and the hash resync runtime.
//!
//! Key details:
//! - The caller selects frame placement and cursor resumption policy.
//! - Each populated string key carries one independent heap owner.

use crate::{Arch, Emitter};

pub const ITER_SOURCE_OFFSET_DELTA: usize = 0;
pub const ITER_CURSOR_OFFSET_DELTA: usize = 8;
pub const ITER_KEY_LO_OFFSET_DELTA: usize = 16;
pub const ITER_KEY_HI_OFFSET_DELTA: usize = 24;
pub const ITER_VALUE_LO_OFFSET_DELTA: usize = 32;
pub const ITER_VALUE_HI_OFFSET_DELTA: usize = 40;
pub const ITER_VALUE_TAG_OFFSET_DELTA: usize = 48;
pub const ITER_VALUE_ADDR_OFFSET_DELTA: usize = 56;
pub const ITER_SNAPSHOT_LEN_OFFSET_DELTA: usize = 64;
/// Table pointer the current associative cursor was computed against.
///
/// `IterNext` compares the live container published by the loop body against this word. A
/// mismatch means growth or a copy-on-write split replaced the table, so the source word is
/// republished before key-identity validation rebuilds the cursor from the successor anchors.
pub const ITER_TABLE_SNAPSHOT_OFFSET_DELTA: usize = 72;
/// Owned keys of the next two entries the cursor can yield, used after table relocation.
///
/// String keys carry one retain per occupied anchor word pair. The primary anchor is the immediate
/// successor. The fallback anchor is its successor, so deleting the immediate successor before a
/// grow still resumes at the first surviving entry. Integer anchors carry no heap ownership.
pub const ITER_NEXT_KEY_LO_OFFSET_DELTA: usize = 80;
pub const ITER_NEXT_KEY_HI_OFFSET_DELTA: usize = 88;
pub const ITER_FALLBACK_KEY_LO_OFFSET_DELTA: usize = 96;
pub const ITER_FALLBACK_KEY_HI_OFFSET_DELTA: usize = 104;
/// Key high word meaning "no successor entry to resume from".
///
/// A live key uses -1 for an integer key and a non-negative length for a string key, so this can
/// never collide with one. Shared by lowering and runtime resync.
pub const NO_SUCCESSOR_KEY_MARKER: i64 = -2;

/// Bytes required for a complete native iterator frame record.
pub const STATE_BYTES: usize = ITER_FALLBACK_KEY_HI_OFFSET_DELTA + 8;

const KEYS: [(usize, usize); 2] = [
    (ITER_NEXT_KEY_LO_OFFSET_DELTA, ITER_NEXT_KEY_HI_OFFSET_DELTA),
    (ITER_FALLBACK_KEY_LO_OFFSET_DELTA, ITER_FALLBACK_KEY_HI_OFFSET_DELTA),
];

/// Initializes both key pairs with the no-successor marker.
pub fn emit_clear_successor_keys<E: Emitter>(
    emitter: &mut E, offset: usize, store: fn(&mut E, &str, usize, &str),
) {
    let value = emitter.secondary_scratch_reg();
    let scratch = emitter.tertiary_scratch_reg();
    for (low, high) in KEYS {
        emitter.emit_load_int_immediate(value, 0);
        store(emitter, value, offset - low, scratch);
        emitter.emit_load_int_immediate(value, NO_SUCCESSOR_KEY_MARKER);
        store(emitter, value, offset - high, scratch);
    }
}

/// Calls a native retain or retirement routine for each populated string-key owner.
pub fn emit_change_key_owners(
    emitter: &mut impl Emitter, offset: usize, done: &[String; 2], operation: &str,
) {
    for ((low, high), done) in KEYS.into_iter().zip(done) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.load_at_offset("x9", offset - high);
                emitter.instruction("cmp x9, #0");                              // identify string keys by a non-negative length
                emitter.instruction(&format!("b.lt {done}"));                   // integer and absent markers own no string
                emitter.load_at_offset("x0", offset - low);
                emitter.instruction(&format!("cbz x0, {done}"));                // skip a cleared pointer
            }
            Arch::X86_64 => {
                emitter.load_at_offset("r10", offset - high);
                emitter.instruction("cmp r10, 0");                              // identify string keys by a non-negative length
                emitter.instruction(&format!("jl {done}"));                     // integer and absent markers own no string
                emitter.load_at_offset("rax", offset - low);
                emitter.instruction("test rax, rax");                           // inspect the pointer before acquiring ownership
                emitter.instruction(&format!("jz {done}"));                     // skip a cleared pointer
            }
        }
        emitter.emit_call_label(operation);
        emitter.label(done);
    }
}
