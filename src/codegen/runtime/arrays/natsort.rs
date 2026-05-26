//! Purpose:
//! Emits the `__rt_natsort`, `__rt_sort_int` runtime helper assembly for natsort.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Sort helpers mutate array payload order in place and must preserve PHP comparison behavior for supported value kinds.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_natsort` and `__rt_natcasesort` runtime helpers.
///
/// On ARM64/macOS: branches to `__rt_sort_int` (integer arrays use numeric order,
/// which satisfies natural-order semantics).
/// On x86_64/Linux: delegates to `emit_natsort_linux_x86_64`.
pub fn emit_natsort(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_natsort_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: natsort (natural order sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natsort");

    // -- for integer arrays, natural sort is numeric sort --
    emitter.instruction("b __rt_sort_int");                                     // tail-call to sort_int (ascending)

    emitter.blank();
    emitter.comment("--- runtime: natcasesort (case-insensitive natural sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natcasesort");

    // -- for integer arrays, case is irrelevant; same as numeric sort --
    emitter.instruction("b __rt_sort_int");                                     // tail-call to sort_int (ascending)
}

/// Emits x86_64/Linux variants of `__rt_natsort` and `__rt_natcasesort`.
///
/// Both labels unconditionally jump to `__rt_sort_int` because integer payloads
/// already satisfy natural-order sorting; case-folding is irrelevant for numeric values.
fn emit_natsort_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: natsort (natural order sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natsort");
    emitter.instruction("jmp __rt_sort_int");                                   // tail-jump to the x86_64 ascending integer sort helper because indexed integers already satisfy natural-order semantics

    emitter.blank();
    emitter.comment("--- runtime: natcasesort (case-insensitive natural sort, delegates to numeric sort) ---");
    emitter.label_global("__rt_natcasesort");
    emitter.instruction("jmp __rt_sort_int");                                   // tail-jump to the same x86_64 ascending integer sort helper because case-folding is irrelevant for integer payloads
}
