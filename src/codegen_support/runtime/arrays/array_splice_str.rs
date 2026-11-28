//! Purpose:
//! Emits `__rt_array_splice_str` and `__rt_array_splice_insert_str`, the `array_splice()` runtime
//! helpers for indexed arrays whose payload slots are string `{pointer, length}` pairs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The EIR lowering of `array_splice()` in `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Indexed string arrays use 16-byte slots (`__rt_array_new(n, 16)`), not the 8-byte slots the
//!   scalar and refcounted splice helpers move. Running those over a string array copied one word
//!   per element, so the removed-elements array came back holding raw pointers read as integers
//!   and the source array kept a half-shifted payload.
//! - The removal MOVES each string slot into the result. An indexed string array owns its
//!   persisted payloads exclusively (`__rt_array_clone_shallow` re-persists them on a
//!   copy-on-write split and `__rt_array_free_deep` frees each one), so transferring the pointer
//!   keeps exactly one owner per string. Retaining or copying here would double-free or leak.
//! - The insertion DUPLICATES each replacement string through `__rt_str_persist`, because the
//!   replacement array keeps owning its own payloads and is released by the caller afterwards.
//! - The destination is grown before anything is written, and the tail slide walks backwards, so
//!   a replacement longer than the removed window never overwrites a slot it has not moved yet.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::array_splice_remove;
pub use elephc_memoir::values::array_splice_insert::emit_array_splice_insert_str;

/// Emits the `__rt_array_splice_str` runtime helper for the active target.
///
/// Removes `$length` string slots starting at `$offset`, returns them in a freshly allocated
/// 16-byte-slot indexed array, and compacts the source payload over the gap.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = source indexed array, `x1` = `$offset`, `x2` = `$length`, `x3` = 1 when a
///   `$length` was supplied and 0 when it was omitted or `null`
/// - **Output**: `x0` = the removed-elements array, `x1` = the normalized removal offset, i.e.
///   the index a `$replacement` is inserted at
///
/// ## x86_64 ABI
/// - **Input**: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning
/// - **Output**: `rax` = the removed-elements array, `rdx` = the normalized removal offset
///
/// The window is normalized by the shared `emit_slice_bounds` prologue, so the removal count is
/// always in `[0, length - offset]` and neither loop can step outside the source payload.
pub fn emit_array_splice_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_splice_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_str ---");
    emitter.label_global("__rt_array_splice_str");

    // Stack layout: [sp,#0] source array, [sp,#8] normalized offset, [sp,#16] removal count,
    //               [sp,#24] result array, [sp,#32] saved x29/x30.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the string-splice bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the source indexed-array pointer across the result-array constructor

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice_str");
    array_splice_remove::emit_remove_strings(emitter);
}

/// Emits the x86_64 System V variant of `__rt_array_splice_str`.
///
/// Mirrors the ARM64 sequence instruction for instruction; only the register encoding differs.
/// See [`emit_array_splice_str`] for the full ABI and the ownership contract.
fn emit_array_splice_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_str ---");
    emitter.label_global("__rt_array_splice_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the string-splice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source, window, and result bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots while keeping the constructor call 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across the result-array constructor

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice_str");
    array_splice_remove::emit_remove_strings(emitter);
}
