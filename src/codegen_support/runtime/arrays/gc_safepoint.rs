//! Purpose:
//! Emits `__rt_gc_safepoint`, the throttle in front of the cycle collector.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The `unset()` lowering in `crate::codegen::lower_inst::core_misc`.
//!
//! Key details:
//! - `unset()` is the ONLY thing that reaches the collector: PHP's own `gc_collect_cycles()` folds
//!   to the constant 0 (`crate::builtins::system::gc`) and calls no helper. So throttling here
//!   cannot make an explicit collection request go unanswered.
//! - `__rt_gc_collect_cycles` is a FOUR-PASS MARK AND SWEEP OVER THE WHOLE HEAP. Running it on
//!   every `unset()` made it the single largest cost in a Symfony request: `sample` attributed
//!   2180 of 6024 main-thread samples to `_rt_gc_collect_cycles` and another 1437 to
//!   `_rt_gc_mark_reachable` — together more than a third of the CPU, against a request that
//!   spends the rest of its time in ordinary compiled Symfony code.
//! - The threshold mirrors PHP's own model. php-src buffers POSSIBLE ROOTS and only collects when
//!   the buffer fills at `GC_THRESHOLD_DEFAULT` (10,000 roots + 1); one `unset()` here is one
//!   possible root, so counting safe points is the same trade php-src makes. Collection timing is
//!   not observable beyond destructor ORDER, which php-src defers in exactly this way.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// How many `unset()` safe points pass before a cycle collection runs.
///
/// Mirrors php-src's `GC_THRESHOLD_DEFAULT`: the collector runs once the possible-root buffer
/// fills, not on every candidate.
pub(crate) const GC_SAFEPOINT_THRESHOLD: i64 = 10_000;

/// Emits the `__rt_gc_safepoint` runtime helper for the active target.
///
/// ## ARM64 / x86_64 ABI
/// - **Input**: none
/// - **Output**: none; clobbers only scratch registers when it does not collect
pub fn emit_gc_safepoint(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gc_safepoint_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gc_safepoint ---");
    emitter.label_global("__rt_gc_safepoint");

    abi::emit_symbol_address(emitter, "x9", "_gc_safepoint_count");
    emitter.instruction("ldr x10, [x9]");                                       // possible roots seen since the last collection
    emitter.instruction("add x10, x10, #1");                                    // this `unset()` is one more
    abi::emit_load_int_immediate(emitter, "x11", GC_SAFEPOINT_THRESHOLD);
    emitter.instruction("cmp x10, x11");                                        // has the buffer filled?
    emitter.instruction("b.hs __rt_gc_safepoint_collect");                      // yes: fall through to a real collection
    emitter.instruction("str x10, [x9]");                                       // no: just record the candidate and return
    emitter.instruction("ret");

    emitter.label("__rt_gc_safepoint_collect");
    emitter.instruction("str xzr, [x9]");                                       // reset the buffer before collecting
    emitter.instruction("b __rt_gc_collect_cycles");                            // tail-call: the collector returns to our caller
}

/// Emits the x86_64 variant of `__rt_gc_safepoint`.
fn emit_gc_safepoint_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_safepoint ---");
    emitter.label_global("__rt_gc_safepoint");

    abi::emit_symbol_address(emitter, "r10", "_gc_safepoint_count");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // possible roots seen since the last collection
    emitter.instruction("add r11, 1");                                          // this `unset()` is one more
    emitter.instruction(&format!("cmp r11, {}", GC_SAFEPOINT_THRESHOLD));       // has the buffer filled?
    emitter.instruction("jae __rt_gc_safepoint_collect");                       // yes: fall through to a real collection
    emitter.instruction("mov QWORD PTR [r10], r11");                            // no: just record the candidate and return
    emitter.instruction("ret");

    emitter.label("__rt_gc_safepoint_collect");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // reset the buffer before collecting
    emitter.instruction("jmp __rt_gc_collect_cycles");                          // tail-call: the collector returns to our caller
}
