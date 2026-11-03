//! Purpose:
//! Emits the `__rt_trim_mask`, `__rt_ltrim_mask` runtime helper assembly for trim mask.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Trim helpers scan byte ranges without copying unless the returned pointer/length slice changes.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// trim_mask: strip characters in mask from both ends of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=adjusted_ptr, x2=adjusted_len
pub fn emit_trim_mask(emitter: &mut Emitter) {
    super::trim_warnings::emit_trim_warning_builder(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_trim_mask_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: trim_mask ---");
    emitter.label_global("__rt_trim_mask");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve mask/name spill slots and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save mask and warning-name args across the nested calls --
    emitter.instruction("stp x3, x4, [sp]");                                    // save mask pointer and length on stack
    emitter.instruction("stp x5, x6, [sp, #16]");                               // save the PHP-visible function name on stack

    // -- ltrim warns once; rtrim re-parses the same mask silently --
    emitter.instruction("mov x7, #1");                                          // enable invalid-range warnings on the first scan
    emitter.instruction("bl __rt_ltrim_mask");                                  // strip leading mask chars (adjusts x1, x2)
    emitter.instruction("ldp x3, x4, [sp]");                                    // restore mask pointer and length
    emitter.instruction("ldp x5, x6, [sp, #16]");                               // restore the PHP-visible function name
    emitter.instruction("mov x7, #0");                                          // suppress duplicate warnings on the second scan
    emitter.instruction("bl __rt_rtrim_mask");                                  // strip trailing mask chars (adjusts x2)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux-specific implementation of `__rt_trim_mask`.
/// Saves the mask pointer/length in red zone slots, delegates to `__rt_ltrim_mask`
/// then `__rt_rtrim_mask`, and restores the frame before returning.
/// The adjusted string pointer/length is returned via the standard x86_64 string ABI.
fn emit_trim_mask_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim_mask ---");
    emitter.label_global("__rt_trim_mask");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving aligned spill space for the trim mask pair
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base so trim_mask() can preserve the mask pointer and length across two helper calls
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the mask, warning name, and lengths
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the trim-mask pointer across the nested ltrim_mask() and rtrim_mask() helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the trim-mask length across the nested ltrim_mask() and rtrim_mask() helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // preserve the PHP-visible function name pointer across the nested calls
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // preserve the PHP-visible function name length across the nested calls
    emitter.instruction("mov r10, 1");                                          // enable invalid-range warnings on the first scan
    emitter.instruction("call __rt_ltrim_mask");                                // trim leading mask bytes first so the borrowed source slice is rebased before the trailing trim pass
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the trim-mask pointer before invoking the trailing trim helper on the adjusted source slice
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the trim-mask length before invoking the trailing trim helper on the adjusted source slice
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // restore the PHP-visible function name pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // restore the PHP-visible function name length
    emitter.instruction("mov r10, 0");                                          // suppress duplicate warnings on the second scan
    emitter.instruction("call __rt_rtrim_mask");                                // trim trailing mask bytes from the already-left-trimmed source slice
    emitter.instruction("add rsp, 48");                                         // release the trim-mask spill slots before returning the adjusted source slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the adjusted source slice
    emitter.instruction("ret");                                                 // return the adjusted borrowed source string slice in the standard x86_64 string result registers
}
