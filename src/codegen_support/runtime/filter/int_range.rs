//! Purpose:
//! Emits the `__rt_filter_int_range` runtime helper backing `FILTER_VALIDATE_INT`
//! with a compile-time `min_range`/`max_range` constraint
//! (`filter_var($v, FILTER_VALIDATE_INT, ['options' => ['min_range' => C, ...]])`).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the filter runtime section.
//!
//! Key details:
//! - Refines an already-computed base `FILTER_VALIDATE_INT` boxed `Mixed` result: a
//!   non-int payload (a `false`/`null` validation failure) passes through unchanged; an
//!   int within the inclusive `[min, max]` bounds passes through unchanged; an
//!   out-of-range int is released and replaced with the range-failure value
//!   (`false`, or `null` when the `_nof` variant set `fail_is_null`).
//! - Bounds are signed 64-bit; absent `min_range`/`max_range` are passed as
//!   `PHP_INT_MIN`/`PHP_INT_MAX` by the caller so both comparisons are unconditional.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_filter_int_range`.
///
/// Input:  AArch64 x0=base Mixed*, x1=min, x2=max, x3=fail_is_null
///         x86_64  rax=base Mixed*, rdi=min, rsi=max, rdx=fail_is_null
/// Output: boxed Mixed pointer in the integer result register (x0 / rax)
pub fn emit_filter_int_range(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_int_range_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_int_range ---");
    emitter.label_global("__rt_filter_int_range");

    emitter.instruction("sub sp, sp, #48");                                     // allocate a helper frame for the base result and bounds
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the base boxed result pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the inclusive lower bound
    emitter.instruction("str x2, [sp, #16]");                                   // save the inclusive upper bound
    emitter.instruction("str x3, [sp, #24]");                                   // save the range-failure mode (1 = null, 0 = false)

    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo for the base result
    emitter.instruction("cmp x0, #0");                                          // is the base result an int?
    emitter.instruction("b.ne __rt_filter_int_range_keep");                     // a non-int failure/null passes through unchanged
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the lower bound
    emitter.instruction("cmp x1, x9");                                          // compare the int value against min_range
    emitter.instruction("b.lt __rt_filter_int_range_fail");                     // below min_range is out of range
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the upper bound
    emitter.instruction("cmp x1, x9");                                          // compare the int value against max_range
    emitter.instruction("b.gt __rt_filter_int_range_fail");                     // above max_range is out of range

    emitter.label("__rt_filter_int_range_keep");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the base result unchanged
    emitter.instruction("b __rt_filter_int_range_done");                        // restore the frame and return

    emitter.label("__rt_filter_int_range_fail");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the out-of-range int box for release
    emitter.instruction("bl __rt_decref_mixed");                               // release the replaced base result
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the range-failure mode
    emitter.instruction("cbz x9, __rt_filter_int_range_false");                 // mode 0 boxes bool(false)
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null (the _nof failure value)
    emitter.instruction("b __rt_filter_int_range_box");                         // box the null failure value
    emitter.label("__rt_filter_int_range_false");
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = bool
    emitter.label("__rt_filter_int_range_box");
    emitter.instruction("mov x1, #0");                                          // payload low word 0 (false / null)
    emitter.instruction("mov x2, #0");                                          // payload high word unused
    emitter.instruction("bl __rt_mixed_from_value");                            // box the range-failure value

    emitter.label("__rt_filter_int_range_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the boxed Mixed result in x0
}

/// Emits the x86_64 Linux variant of `__rt_filter_int_range` (System V AMD64 ABI).
fn emit_filter_int_range_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_int_range ---");
    emitter.label_global("__rt_filter_int_range");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame base
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for the base result and bounds
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the base boxed result pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the inclusive lower bound
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the inclusive upper bound
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the range-failure mode (1 = null, 0 = false)

    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=value_lo for the base result
    emitter.instruction("cmp rax, 0");                                          // is the base result an int?
    emitter.instruction("jne __rt_filter_int_range_keep_x86_64");               // a non-int failure/null passes through unchanged
    emitter.instruction("mov rcx, rdi");                                        // rcx = the unboxed int value
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare against min_range
    emitter.instruction("jl __rt_filter_int_range_fail_x86_64");                // below min_range is out of range
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // compare against max_range
    emitter.instruction("jg __rt_filter_int_range_fail_x86_64");                // above max_range is out of range

    emitter.label("__rt_filter_int_range_keep_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the base result unchanged
    emitter.instruction("jmp __rt_filter_int_range_done_x86_64");               // restore the frame and return

    emitter.label("__rt_filter_int_range_fail_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the out-of-range int box for release
    emitter.instruction("call __rt_decref_mixed");                             // release the replaced base result
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // check the range-failure mode
    emitter.instruction("je __rt_filter_int_range_false_x86_64");               // mode 0 boxes bool(false)
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null (the _nof failure value)
    emitter.instruction("jmp __rt_filter_int_range_box_x86_64");                // box the null failure value
    emitter.label("__rt_filter_int_range_false_x86_64");
    emitter.instruction("mov rax, 3");                                          // runtime tag 3 = bool
    emitter.label("__rt_filter_int_range_box_x86_64");
    emitter.instruction("xor rdi, rdi");                                        // payload low word 0 (false / null)
    emitter.instruction("xor rsi, rsi");                                        // payload high word unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the range-failure value

    emitter.label("__rt_filter_int_range_done_x86_64");
    emitter.instruction("add rsp, 48");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed result in rax
}

#[cfg(test)]
mod tests {
    use crate::codegen::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the AArch64 helper emits the global symbol, the pass-through/keep
    /// path, and the release + range-failure boxing path.
    #[test]
    fn test_emit_filter_int_range_arm64() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_filter_int_range(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_filter_int_range:\n"));
        assert!(asm.contains("bl __rt_mixed_unbox"));
        assert!(asm.contains("bl __rt_decref_mixed"));
        assert!(asm.contains("bl __rt_mixed_from_value"));
        assert!(asm.contains("b.lt __rt_filter_int_range_fail"));
    }

    /// Verifies the x86_64 helper emits the range compares and the failure boxing.
    #[test]
    fn test_emit_filter_int_range_linux_x86_64() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_filter_int_range(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_filter_int_range:\n"));
        assert!(asm.contains("call __rt_decref_mixed"));
        assert!(asm.contains("jl __rt_filter_int_range_fail_x86_64"));
        assert!(asm.contains("jg __rt_filter_int_range_fail_x86_64"));
    }
}
