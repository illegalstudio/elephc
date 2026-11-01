//! Purpose:
//! Emits copy-on-write byte-string offset assignment for PHP strings.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the string runtime.
//!
//! Key details:
//! - Negative offsets are relative to the source length and out-of-range negative writes are no-ops.
//! - Positive offsets extend the result with ASCII spaces before storing the first replacement byte.

use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_str_set_offset` copy-on-write helper.
///
/// AArch64 receives `(x1,x2,x0,x3,x4)` and returns `(x1,x2)`; x86_64 receives
/// `(rax,rdx,r8,rdi,rsi)` and returns `(rax,rdx)`.
pub fn emit_str_set_offset(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_set_offset_x86_64(emitter);
    } else {
        emit_str_set_offset_aarch64(emitter);
    }
}

/// Emits PHP byte-string offset assignment for AArch64 targets.
fn emit_str_set_offset_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: string offset assignment ---");
    emitter.label_global("__rt_str_set_offset");
    emitter.instruction("sub sp, sp, #96");                                    // reserve source, replacement, result metadata, and the saved frame
    emitter.instruction("stp x29, x30, [sp, #80]");                            // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                   // establish a stable helper frame
    emitter.instruction("stp x1, x2, [sp]");                                   // preserve the source string pointer and length
    emitter.instruction("str x0, [sp, #16]");                                  // preserve the requested signed byte offset
    emitter.instruction("stp x3, x4, [sp, #24]");                              // preserve the replacement string pointer and length
    emitter.instruction("mov x9, #1");                                         // assume the offset and replacement select a byte to write
    emitter.instruction("str x9, [sp, #64]");                                  // preserve the write-valid flag across allocation
    emitter.instruction("cbz x4, __rt_str_set_offset_invalid");                // an empty replacement cannot provide an assigned byte
    emitter.instruction("cmp x0, #0");                                         // distinguish absolute and length-relative offsets
    emitter.instruction("b.ge __rt_str_set_offset_index_ready");               // keep non-negative offsets unchanged
    emitter.instruction("add x0, x2, x0");                                     // translate a negative offset relative to the source end
    emitter.instruction("cmp x0, #0");                                         // check whether the relative offset still precedes the source
    emitter.instruction("b.lt __rt_str_set_offset_invalid");                   // invalid negative offsets leave the source bytes unchanged
    emitter.instruction("b __rt_str_set_offset_index_ready");                  // continue with the normalized in-range offset
    emitter.label("__rt_str_set_offset_invalid");
    emitter.instruction("mov x9, #0");                                         // suppress the replacement write for an invalid request
    emitter.instruction("str x9, [sp, #64]");                                  // publish the invalid-request flag to the final write step
    emitter.instruction("mov x0, #0");                                         // use a harmless offset while copying the unchanged source
    emitter.label("__rt_str_set_offset_index_ready");
    emitter.instruction("str x0, [sp, #40]");                                  // preserve the normalized replacement offset
    emitter.instruction("ldr x9, [sp, #64]");                                  // reload whether this request can write a byte
    emitter.instruction("cbz x9, __rt_str_set_offset_length_source");          // invalid requests retain the original source length
    emitter.instruction("adds x10, x0, #1");                                   // include the replacement byte in the required result length
    emitter.instruction("b.vc __rt_str_set_offset_length_no_overflow");        // continue when offset-plus-one remains a positive signed length
    emitter.instruction("b __rt_alloc_overflow");                              // route an overflowing required length through the shared fatal path
    emitter.label("__rt_str_set_offset_length_no_overflow");
    emitter.instruction("cmp x2, x10");                                        // compare source length with offset-plus-one
    emitter.instruction("csel x10, x2, x10, hs");                              // preserve the source or extend through the assigned offset
    emitter.instruction("b __rt_str_set_offset_length_ready");                 // skip the invalid-request source-length fallback
    emitter.label("__rt_str_set_offset_length_source");
    emitter.instruction("mov x10, x2");                                        // invalid requests make an unchanged copy
    emitter.label("__rt_str_set_offset_length_ready");
    emitter.instruction("str x10, [sp, #48]");                                 // preserve the selected result length
    emitter.instruction("mov x0, x10");                                        // request bounded destination storage
    emitter.instruction("bl __rt_concat_reserve");                             // reserve scratch or heap-backed string storage
    emitter.instruction("str x0, [sp, #56]");                                  // preserve the destination base across the byte loops
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x0, x9");                                    // derive a candidate offset inside concat scratch
    abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity as i64); // load the concat scratch capacity
    emitter.instruction("cmp x10, x11");                                       // determine whether the reservation is heap-backed
    emitter.instruction("b.lo __rt_str_set_offset_dest_ready");                // scratch-backed results have no heap header
    emitter.instruction(&format!("mov x11, #{}", CONCAT_TEMP_HEAP_KIND));      // mark an unowned transient string result
    emitter.instruction("str x11, [x0, #-8]");                                 // let string persistence take over a heap fallback in place
    emitter.label("__rt_str_set_offset_dest_ready");
    emitter.instruction("mov x9, #0");                                         // initialize the source-copy index
    emitter.label("__rt_str_set_offset_copy");
    emitter.instruction("ldr x10, [sp, #8]");                                  // reload the source length
    emitter.instruction("cmp x9, x10");                                        // test whether every source byte was copied
    emitter.instruction("b.hs __rt_str_set_offset_fill");                      // continue by filling any positive-offset extension
    emitter.instruction("ldr x11, [sp]");                                      // reload the source byte pointer
    emitter.instruction("ldrb w12, [x11, x9]");                                // load one source byte
    emitter.instruction("ldr x11, [sp, #56]");                                 // reload the destination base
    emitter.instruction("strb w12, [x11, x9]");                                // copy the source byte into independent result storage
    emitter.instruction("add x9, x9, #1");                                     // advance to the next source byte
    emitter.instruction("b __rt_str_set_offset_copy");                         // continue until the source prefix is complete
    emitter.label("__rt_str_set_offset_fill");
    emitter.instruction("ldr x10, [sp, #48]");                                 // reload the selected result length
    emitter.instruction("cmp x9, x10");                                        // check whether an extension gap remains
    emitter.instruction("b.hs __rt_str_set_offset_store");                     // proceed once every result byte is initialized
    emitter.instruction("ldr x11, [sp, #56]");                                 // reload the destination base for gap filling
    emitter.instruction("mov w12, #32");                                       // PHP fills positive-offset gaps with ASCII spaces
    emitter.instruction("strb w12, [x11, x9]");                                // initialize the current extension byte
    emitter.instruction("add x9, x9, #1");                                     // advance through the extension gap
    emitter.instruction("b __rt_str_set_offset_fill");                         // continue until the destination length is initialized
    emitter.label("__rt_str_set_offset_store");
    emitter.instruction("ldr x9, [sp, #64]");                                  // reload whether a replacement byte should be stored
    emitter.instruction("cbz x9, __rt_str_set_offset_done");                   // invalid requests return the unchanged copied bytes
    emitter.instruction("ldr x10, [sp, #24]");                                 // reload the replacement byte pointer
    emitter.instruction("ldrb w11, [x10]");                                    // PHP assigns only the first replacement byte
    emitter.instruction("ldr x10, [sp, #56]");                                 // reload the destination base
    emitter.instruction("ldr x9, [sp, #40]");                                  // reload the normalized destination offset
    emitter.instruction("strb w11, [x10, x9]");                                // replace the selected byte in the independent copy
    emitter.label("__rt_str_set_offset_done");
    emitter.instruction("ldr x1, [sp, #56]");                                  // return the copied result pointer in the string ABI
    emitter.instruction("ldr x2, [sp, #48]");                                  // return the copied or extended result length
    emitter.instruction("bl __rt_concat_publish");                             // publish only scratch-backed bytes
    emitter.instruction("ldp x29, x30, [sp, #80]");                            // restore the caller frame
    emitter.instruction("add sp, sp, #96");                                    // release helper-local storage
    emitter.instruction("ret");                                                // return the PHP string pair
}

/// Emits PHP byte-string offset assignment for x86_64 System V targets.
fn emit_str_set_offset_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: string offset assignment ---");
    emitter.label_global("__rt_str_set_offset");
    emitter.instruction("push rbp");                                           // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                       // establish a stable helper frame
    emitter.instruction("sub rsp, 80");                                        // reserve source, replacement, and result metadata
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // preserve the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                      // preserve the source string length
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                       // preserve the requested signed byte offset
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                      // preserve the replacement string pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                      // preserve the replacement string length
    emitter.instruction("mov QWORD PTR [rbp - 72], 1");                        // assume the request selects a replacement byte
    emitter.instruction("cmp rsi, 0");                                         // check whether the replacement provides a first byte
    emitter.instruction("je __rt_str_set_offset_invalid_x86");                 // empty replacements leave the source bytes unchanged
    emitter.instruction("cmp r8, 0");                                          // distinguish absolute and length-relative offsets
    emitter.instruction("jge __rt_str_set_offset_index_ready_x86");            // keep non-negative offsets unchanged
    emitter.instruction("add r8, rdx");                                        // translate a negative offset relative to the source end
    emitter.instruction("cmp r8, 0");                                          // check whether the relative offset still precedes the source
    emitter.instruction("jl __rt_str_set_offset_invalid_x86");                 // invalid negative offsets leave the source bytes unchanged
    emitter.instruction("jmp __rt_str_set_offset_index_ready_x86");            // continue with the normalized in-range offset
    emitter.label("__rt_str_set_offset_invalid_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                        // suppress the replacement write for an invalid request
    emitter.instruction("xor r8d, r8d");                                       // use a harmless offset while copying the unchanged source
    emitter.label("__rt_str_set_offset_index_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                       // preserve the normalized replacement offset
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                       // reload whether this request can write a byte
    emitter.instruction("je __rt_str_set_offset_length_source_x86");           // invalid requests retain the original source length
    emitter.instruction("mov r9, r8");                                         // seed the required length from the normalized offset
    emitter.instruction("add r9, 1");                                          // include the replacement byte in the required result length
    emitter.instruction("jo __rt_alloc_overflow");                             // reject a signed offset whose required length overflows
    emitter.instruction("cmp rdx, r9");                                        // compare source length with offset-plus-one
    emitter.instruction("cmova r9, rdx");                                      // preserve the source or extend through the assigned offset
    emitter.instruction("jmp __rt_str_set_offset_length_ready_x86");           // skip the invalid-request source-length fallback
    emitter.label("__rt_str_set_offset_length_source_x86");
    emitter.instruction("mov r9, rdx");                                        // invalid requests make an unchanged copy
    emitter.label("__rt_str_set_offset_length_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                       // preserve the selected result length
    emitter.instruction("mov rax, r9");                                        // request bounded destination storage
    emitter.instruction("call __rt_concat_reserve");                           // reserve scratch or heap-backed string storage
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                      // preserve the destination base across the byte loops
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov r10, rax");                                       // copy the destination before deriving its scratch offset
    emitter.instruction("sub r10, r9");                                        // derive a candidate offset inside concat scratch
    emitter.instruction(&format!("cmp r10, {}", emitter.concat_capacity));         // determine whether the reservation is heap-backed
    emitter.instruction("jb __rt_str_set_offset_dest_ready_x86");              // scratch-backed results have no heap header
    emitter.instruction(&format!("mov r9, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND))); // mark an unowned transient string result
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                        // let string persistence take over a heap fallback in place
    emitter.label("__rt_str_set_offset_dest_ready_x86");
    emitter.instruction("xor r9d, r9d");                                       // initialize the source-copy index
    emitter.label("__rt_str_set_offset_copy_x86");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                       // test whether every source byte was copied
    emitter.instruction("jae __rt_str_set_offset_fill_x86");                   // continue by filling any positive-offset extension
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                       // reload the source byte pointer
    emitter.instruction("movzx ecx, BYTE PTR [r10 + r9]");                     // load one source byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                      // reload the destination base
    emitter.instruction("mov BYTE PTR [r10 + r9], cl");                        // copy the source byte into independent result storage
    emitter.instruction("add r9, 1");                                          // advance to the next source byte
    emitter.instruction("jmp __rt_str_set_offset_copy_x86");                   // continue until the source prefix is complete
    emitter.label("__rt_str_set_offset_fill_x86");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 56]");                       // check whether an extension gap remains
    emitter.instruction("jae __rt_str_set_offset_store_x86");                  // proceed once every result byte is initialized
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                      // reload the destination base for gap filling
    emitter.instruction("mov BYTE PTR [r10 + r9], 32");                        // PHP fills positive-offset gaps with ASCII spaces
    emitter.instruction("add r9, 1");                                          // advance through the extension gap
    emitter.instruction("jmp __rt_str_set_offset_fill_x86");                   // continue until the destination length is initialized
    emitter.label("__rt_str_set_offset_store_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                       // reload whether a replacement byte should be stored
    emitter.instruction("je __rt_str_set_offset_done_x86");                    // invalid requests return the unchanged copied bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                      // reload the replacement byte pointer
    emitter.instruction("movzx ecx, BYTE PTR [r10]");                          // PHP assigns only the first replacement byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                      // reload the destination base
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                       // reload the normalized destination offset
    emitter.instruction("mov BYTE PTR [r10 + r9], cl");                        // replace the selected byte in the independent copy
    emitter.label("__rt_str_set_offset_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                      // return the copied result pointer in the string ABI
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                      // return the copied or extended result length
    emitter.instruction("call __rt_concat_publish");                           // publish only scratch-backed bytes
    emitter.instruction("leave");                                              // restore the caller stack and frame pointer
    emitter.instruction("ret");                                                // return the PHP string pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies that both supported instruction-set emitters include copy, fill, and write paths.
    #[test]
    fn test_emit_str_set_offset_covers_both_target_abis() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_str_set_offset(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains("__rt_str_set_offset_copy"));
        assert!(arm_asm.contains("strb w11, [x10, x9]"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_str_set_offset(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains("__rt_str_set_offset_copy_x86"));
        assert!(x86_asm.contains("mov BYTE PTR [r10 + r9], cl"));
    }
}
