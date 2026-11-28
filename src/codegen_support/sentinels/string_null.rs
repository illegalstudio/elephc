//! Purpose:
//! Classifies logical PHP null carried by typed string storage.
//!
//! Called from:
//! - Mixed boxing/unboxing and PHP value-kind predicates on both architectures.
//!
//! Key details:
//! - Only the dedicated missing-string pointer denotes null; a zero-length string stays a string.
//! - Integer and floating-point payloads never enter this pointer classification.

use super::{Arch, Emitter, NULL_SENTINEL};
use crate::codegen_support::abi;

/// Compares a string pointer to its missing-value marker, clobbering only the scratch register.
pub(crate) fn emit_compare_null_string(emitter: &mut Emitter, pointer: &str, scratch: &str) {
    abi::emit_load_int_immediate(emitter, scratch, NULL_SENTINEL);
    emitter.instruction(&format!("cmp {pointer}, {scratch}"));                  // distinguish a missing string from a genuine empty string
}

/// Branches on a logical null string while preserving genuine empty pointer/length pairs.
pub(crate) fn emit_branch_if_null_string(
    emitter: &mut Emitter, pointer: &str, scratch: &str, label: &str,
) {
    emit_compare_null_string(emitter, pointer, scratch);
    abi::emit_branch_if_equal_wide(emitter, label);
}

/// Normalizes a loaded boxed string tag without mutating the borrowed cell.
/// The cell register becomes its string pointer; the scratch register is clobbered.
pub(crate) fn emit_normalize_boxed_string_tag(
    emitter: &mut Emitter, tag: &str, cell: &str, scratch: &str, done: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #1"));                     // only a string tag can carry the missing-string marker
            abi::emit_branch_if_not_equal_wide(emitter, done);
            emitter.instruction(&format!("ldr {cell}, [{cell}, #8]"));          // inspect the borrowed string pointer without acquiring it
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, 1"));                      // only a string tag can carry the missing-string marker
            abi::emit_branch_if_not_equal_wide(emitter, done);
            emitter.instruction(&format!("mov {cell}, QWORD PTR [{cell} + 8]")); // inspect the borrowed string pointer without acquiring it
        }
    }
    emit_compare_null_string(emitter, cell, scratch);
    abi::emit_load_int_immediate(emitter, scratch, 8);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("csel {tag}, {scratch}, {tag}, eq")), // report PHP null only for the missing-string marker
        Arch::X86_64 => emitter.instruction(&format!("cmove {tag}, {scratch}")),  // report PHP null only for the missing-string marker
    }
    emitter.label(done);
}
