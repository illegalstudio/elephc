//! Purpose:
//! Emits the `__rt_asort`, `__rt_sort_int` runtime helper assembly for asort.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Sort helpers mutate array payload order in place and must preserve PHP comparison behavior for supported value kinds.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_asort` and `__rt_arsort` runtime helpers for sorting arrays by value.
///
/// For indexed integer arrays, delegates directly to `__rt_sort_int` / `__rt_rsort_int`
/// since slot order semantics are equivalent when no string keys need preservation.
///
/// - Input: `x0` = array pointer
/// - Output: array sorted in-place (mutates caller's array)
/// - ABI: ARM64 uses `b` (branch-and-link), x86_64 uses `jmp` (tail-jump)
pub fn emit_asort(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_asort_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: asort (sort by values ascending) ---");
    emitter.label_global("__rt_asort");

    // -- delegate to existing ascending integer sort --
    emitter.instruction("b __rt_sort_int");                                     // tail-call to sort_int (ascending)

    emitter.blank();
    emitter.comment("--- runtime: arsort (sort by values descending) ---");
    emitter.label_global("__rt_arsort");

    // -- delegate to existing descending integer sort --
    emitter.instruction("b __rt_rsort_int");                                    // tail-call to rsort_int (descending)
}

/// x86_64-specific emitter for `__rt_asort` and `__rt_arsort`.
///
/// Uses `jmp` (tail-jump) instead of ARM64's `b` because x86_64 lacks a direct
/// branch-and-link equivalent; tail-jumping preserves the caller-saved register contract.
fn emit_asort_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: asort (sort by values ascending) ---");
    emitter.label_global("__rt_asort");
    emitter.instruction("jmp __rt_sort_int");                                   // tail-jump to the x86_64 ascending integer sort helper because indexed arrays already preserve slot order semantics

    emitter.blank();
    emitter.comment("--- runtime: arsort (sort by values descending) ---");
    emitter.label_global("__rt_arsort");
    emitter.instruction("jmp __rt_rsort_int");                                  // tail-jump to the x86_64 descending integer sort helper because indexed arrays already preserve slot order semantics
}
