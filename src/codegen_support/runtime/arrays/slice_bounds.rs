//! Purpose:
//! Emits the shared PHP `$offset`/`$length` normalization prologue used by every slice-like
//! runtime helper, indexed or hash-backed.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays::array_slice`,
//!   `array_slice_refcounted`, `array_slice_to_hash`, `array_splice`,
//!   `array_splice_refcounted`, `array_splice_str` and `hash_slice`.
//!
//! Key details:
//! - There is no out-of-band `i64` a PHP `$length` cannot take, so "no `$length` given" travels in a
//!   dedicated fourth argument register instead of a magic length value. `-1` used to double as the
//!   until-the-end sentinel, which collided with PHP's `-1` = "stop one element before the end".
//! - The emitted sequence is the single source of truth for PHP's slice window arithmetic, so the
//!   scalar and refcounted slice/splice helpers cannot drift apart.
//! - Every clamp is signed and the result window is always inside `[0, length]`, so no caller can
//!   publish a negative logical length or copy outside the source payload.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits PHP's `array_slice`/`array_splice` `$offset`/`$length` normalization.
///
/// `prefix` seeds the local label names, so each helper that inlines this sequence must pass its own
/// unique string.
///
/// # ABI
/// - **ARM64** — in: `x0` = source array pointer, `x1` = raw `$offset`, `x2` = raw `$length`,
///   `x3` = 1 when the caller passed a `$length` and 0 when it was omitted or `null`.
///   Out: `x1` = normalized offset in `[0, n]`, `x2` = clamped window length in `[0, n - offset]`,
///   `x9` = source length `n`, `x10` = elements available from the normalized offset. Clobbers
///   `x9`/`x10`; `x0` is preserved.
/// - **x86_64** — in: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning.
///   Out: `rsi`/`rdx` normalized as above, `r10` = `n`, `r11` = available elements. Clobbers
///   `r10`/`r11`; `rdi` is preserved.
///
/// # Semantics
/// - A negative `$offset` counts backwards from the end and clamps to the start; a too-large
///   `$offset` clamps to the end, which yields an empty window instead of a negative one.
/// - An omitted `$length` takes every remaining element.
/// - A negative `$length` stops that many elements before the end of the source, clamped to an empty
///   window when it would run past the normalized offset.
/// - A positive `$length` is clamped to the elements actually available.
///
/// Neither `n + $offset` nor `available + $length` can overflow: both add a non-negative value to a
/// negative one, and signed overflow needs matching signs.
pub fn emit_slice_bounds(emitter: &mut Emitter, prefix: &str) {
    if emitter.target.arch == Arch::X86_64 {
        emit_slice_bounds_x86_64(emitter, prefix);
        return;
    }

    emitter.comment("-- normalize the PHP slice window: offset, then length --");
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source indexed-array logical length
    emitter.instruction("cmp x1, #0");                                          // does the caller count the offset backwards from the end?
    emitter.instruction(&format!("b.ge {}_off_fwd", prefix));                   // forward offsets only need the upper clamp
    emitter.instruction("add x1, x9, x1");                                      // offset = length + offset for backward offsets
    emitter.instruction("cmp x1, #0");                                          // did the backward offset run past the start of the source?
    emitter.instruction("csel x1, xzr, x1, lt");                                // clamp a too-far backward offset to the first element
    emitter.instruction(&format!("b {}_off_ready", prefix));                    // the backward offset is now inside the source bounds
    emitter.label(&format!("{}_off_fwd", prefix));
    emitter.instruction("cmp x1, x9");                                          // does the forward offset start past the end of the source?
    emitter.instruction("csel x1, x9, x1, gt");                                 // clamp a too-large offset to the end so the window stays empty
    emitter.label(&format!("{}_off_ready", prefix));
    emitter.instruction("sub x10, x9, x1");                                     // x10 = elements available from the normalized offset
    emitter.instruction(&format!("cbz x3, {}_len_absent", prefix));             // an omitted length takes every remaining element
    emitter.instruction("cmp x2, #0");                                          // does the caller stop a number of elements before the end?
    emitter.instruction(&format!("b.lt {}_len_back", prefix));                  // negative lengths are counted back from the source end
    emitter.instruction("cmp x2, x10");                                         // does the requested length run past the available elements?
    emitter.instruction("csel x2, x10, x2, gt");                                // clamp the requested length to the available elements
    emitter.instruction(&format!("b {}_len_ready", prefix));                    // the requested length now fits the source window
    emitter.label(&format!("{}_len_back", prefix));
    emitter.instruction("add x2, x2, x10");                                     // length = available + negative length (operands differ in sign, cannot overflow)
    emitter.instruction("cmp x2, #0");                                          // did the backward length consume more than the window holds?
    emitter.instruction("csel x2, xzr, x2, lt");                                // an over-large backward length yields an empty window, never a negative one
    emitter.instruction(&format!("b {}_len_ready", prefix));                    // the backward length is now a non-negative element count
    emitter.label(&format!("{}_len_absent", prefix));
    emitter.instruction("mov x2, x10");                                         // no length given: take every element from the normalized offset
    emitter.label(&format!("{}_len_ready", prefix));
}

/// Emits the x86_64 variant of the shared slice-window normalization.
///
/// Mirrors the ARM64 sequence instruction for instruction; only the System V register names and the
/// branch-based clamps differ. See [`emit_slice_bounds`] for the full ABI and semantics.
fn emit_slice_bounds_x86_64(emitter: &mut Emitter, prefix: &str) {
    emitter.comment("-- normalize the PHP slice window: offset, then length --");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // r10 = source indexed-array logical length
    emitter.instruction("cmp rsi, 0");                                          // does the caller count the offset backwards from the end?
    emitter.instruction(&format!("jge {}_off_fwd_x86", prefix));                // forward offsets only need the upper clamp
    emitter.instruction("add rsi, r10");                                        // offset = length + offset for backward offsets
    emitter.instruction("cmp rsi, 0");                                          // did the backward offset run past the start of the source?
    emitter.instruction(&format!("jge {}_off_ready_x86", prefix));              // keep the backward offset once it is inside the source bounds
    emitter.instruction("xor esi, esi");                                        // clamp a too-far backward offset to the first element
    emitter.instruction(&format!("jmp {}_off_ready_x86", prefix));              // the backward offset is now inside the source bounds
    emitter.label(&format!("{}_off_fwd_x86", prefix));
    emitter.instruction("cmp rsi, r10");                                        // does the forward offset start past the end of the source?
    emitter.instruction(&format!("jle {}_off_ready_x86", prefix));              // keep the forward offset when it still points inside the source
    emitter.instruction("mov rsi, r10");                                        // clamp a too-large offset to the end so the window stays empty
    emitter.label(&format!("{}_off_ready_x86", prefix));
    emitter.instruction("mov r11, r10");                                        // seed the availability scratch register from the source length
    emitter.instruction("sub r11, rsi");                                        // r11 = elements available from the normalized offset
    emitter.instruction("test rcx, rcx");                                       // did the caller pass an explicit length at all?
    emitter.instruction(&format!("je {}_len_absent_x86", prefix));              // an omitted length takes every remaining element
    emitter.instruction("cmp rdx, 0");                                          // does the caller stop a number of elements before the end?
    emitter.instruction(&format!("jl {}_len_back_x86", prefix));                // negative lengths are counted back from the source end
    emitter.instruction("cmp rdx, r11");                                        // does the requested length run past the available elements?
    emitter.instruction(&format!("jle {}_len_ready_x86", prefix));              // keep the requested length when it fits the available elements
    emitter.instruction("mov rdx, r11");                                        // clamp the requested length to the available elements
    emitter.instruction(&format!("jmp {}_len_ready_x86", prefix));              // the requested length now fits the source window
    emitter.label(&format!("{}_len_back_x86", prefix));
    emitter.instruction("add rdx, r11");                                        // length = available + negative length (operands differ in sign, cannot overflow)
    emitter.instruction("cmp rdx, 0");                                          // did the backward length consume more than the window holds?
    emitter.instruction(&format!("jge {}_len_ready_x86", prefix));              // keep a backward length that still selects at least one element
    emitter.instruction("xor edx, edx");                                        // an over-large backward length yields an empty window, never a negative one
    emitter.instruction(&format!("jmp {}_len_ready_x86", prefix));              // the backward length is now a non-negative element count
    emitter.label(&format!("{}_len_absent_x86", prefix));
    emitter.instruction("mov rdx, r11");                                        // no length given: take every element from the normalized offset
    emitter.label(&format!("{}_len_ready_x86", prefix));
}
