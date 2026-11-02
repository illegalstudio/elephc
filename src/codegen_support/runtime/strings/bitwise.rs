//! Purpose:
//! Emits PHP byte-string bitwise operations for `&`, `|`, and `^`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the string runtime.
//!
//! Key details:
//! - AND and XOR stop at the shorter operand; OR keeps the longer operand's untouched tail.
//! - Results use bounded concat scratch storage and the normal pointer/length string ABI.

use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_str_bitwise` byte-string helper.
///
/// Operation codes are 0 for AND, 1 for OR, and 2 for XOR. AArch64 receives
/// `(x1,x2,x3,x4,x5)` and returns `(x1,x2)`; x86_64 receives
/// `(rax,rdx,rdi,rsi,r8)` and returns `(rax,rdx)`.
pub fn emit_str_bitwise(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_bitwise_x86_64(emitter);
    } else {
        emit_str_bitwise_aarch64(emitter);
    }
}

/// Emits PHP byte-string bitwise operations for AArch64 targets.
fn emit_str_bitwise_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: string bitwise ---");
    emitter.label_global("__rt_str_bitwise");
    emitter.instruction("sub sp, sp, #80");                                     // reserve source pairs, result metadata, and the saved frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save the caller frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the left string pointer and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // preserve the right string pointer and length
    emitter.instruction("str x5, [sp, #32]");                                   // preserve the requested bitwise operation
    emitter.instruction("cmp x2, x4");                                          // compare operand lengths for min/max selection
    emitter.instruction("csel x6, x2, x4, lo");                                 // x6 = shorter length used by AND and XOR
    emitter.instruction("csel x7, x2, x4, hs");                                 // x7 = longer length used by OR
    emitter.instruction("cmp x5, #1");                                          // operation 1 is byte-string OR
    emitter.instruction("csel x6, x7, x6, eq");                                 // OR keeps the longer tail; AND/XOR stop at the shorter input
    emitter.instruction("str x6, [sp, #40]");                                   // preserve the selected result length
    emitter.instruction("mov x0, x6");                                          // request exactly enough bounded result storage
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap-backed string storage
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the result base across the byte loop

    abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("sub x8, x0, x7");                                      // derive a candidate offset inside concat scratch
    abi::emit_load_int_immediate(emitter, "x9", emitter.concat_capacity as i64); // load the concat scratch capacity
    emitter.instruction("cmp x8, x9");                                          // determine whether the reservation is heap-backed
    emitter.instruction("b.lo __rt_str_bitwise_dest_ready");                    // scratch-backed results have no heap header
    emitter.instruction(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND));        // mark an unowned transient string result
    emitter.instruction("str x9, [x0, #-8]");                                   // let string persistence take over a heap fallback in place
    emitter.label("__rt_str_bitwise_dest_ready");

    emitter.instruction("mov x9, #0");                                          // initialize the byte index
    emitter.label("__rt_str_bitwise_loop");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload the result length
    emitter.instruction("cmp x9, x6");                                          // test whether every result byte has been produced
    emitter.instruction("b.hs __rt_str_bitwise_done");                          // finish at the PHP-selected result length
    emitter.instruction("mov w10, #0");                                         // absent left tail bytes contribute zero to OR
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the left length
    emitter.instruction("cmp x9, x2");                                          // check whether the left operand has this byte
    emitter.instruction("b.hs __rt_str_bitwise_left_ready");                    // keep zero beyond the left operand
    emitter.instruction("ldr x1, [sp]");                                        // reload the left byte-string pointer
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the current left byte
    emitter.label("__rt_str_bitwise_left_ready");
    emitter.instruction("mov w11, #0");                                         // absent right tail bytes contribute zero to OR
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the right length
    emitter.instruction("cmp x9, x4");                                          // check whether the right operand has this byte
    emitter.instruction("b.hs __rt_str_bitwise_right_ready");                   // keep zero beyond the right operand
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the right byte-string pointer
    emitter.instruction("ldrb w11, [x3, x9]");                                  // load the current right byte
    emitter.label("__rt_str_bitwise_right_ready");
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload the operation selector
    emitter.instruction("cmp x5, #0");                                          // operation 0 selects AND
    emitter.instruction("b.eq __rt_str_bitwise_and");                           // branch to byte-wise AND
    emitter.instruction("cmp x5, #1");                                          // operation 1 selects OR
    emitter.instruction("b.eq __rt_str_bitwise_or");                            // branch to byte-wise OR
    emitter.instruction("eor w10, w10, w11");                                   // operation 2 computes byte-wise XOR
    emitter.instruction("b __rt_str_bitwise_store");                            // store the XOR result byte
    emitter.label("__rt_str_bitwise_and");
    emitter.instruction("and w10, w10, w11");                                   // compute byte-wise AND
    emitter.instruction("b __rt_str_bitwise_store");                            // store the AND result byte
    emitter.label("__rt_str_bitwise_or");
    emitter.instruction("orr w10, w10, w11");                                   // compute byte-wise OR, including the longer tail
    emitter.label("__rt_str_bitwise_store");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the result base pointer
    emitter.instruction("strb w10, [x0, x9]");                                  // write the current result byte
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_str_bitwise_loop");                             // continue until the selected result length

    emitter.label("__rt_str_bitwise_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // return the result pointer in the string ABI
    emitter.instruction("ldr x2, [sp, #40]");                                   // return the selected result length
    emitter.instruction("bl __rt_concat_publish");                              // publish only scratch-backed bytes
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame
    emitter.instruction("add sp, sp, #80");                                     // release helper-local storage
    emitter.instruction("ret");                                                 // return the PHP string pair
}

/// Emits PHP byte-string bitwise operations for x86_64 System V targets.
fn emit_str_bitwise_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: string bitwise ---");
    emitter.label_global("__rt_str_bitwise");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve source pairs, selector, and result metadata
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the left string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the right string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the right string length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve the requested bitwise operation
    emitter.instruction("mov r9, rdx");                                         // seed the shorter length from the left operand
    emitter.instruction("cmp rdx, rsi");                                        // compare operand lengths
    emitter.instruction("cmova r9, rsi");                                       // r9 = shorter length
    emitter.instruction("cmp r8, 1");                                           // operation 1 is byte-string OR
    emitter.instruction("jne __rt_str_bitwise_length_ready_x86");               // AND and XOR already use the shorter length
    emitter.instruction("mov r9, rdx");                                         // seed OR length from the left operand
    emitter.instruction("cmp rdx, rsi");                                        // compare operand lengths again for maximum selection
    emitter.instruction("cmovb r9, rsi");                                       // OR keeps the longer operand's tail
    emitter.label("__rt_str_bitwise_length_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve the selected result length
    emitter.instruction("mov rax, r9");                                         // request exactly enough bounded result storage
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap-backed string storage
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the result base across the byte loop

    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov r10, rax");                                        // copy the destination before deriving its scratch offset
    emitter.instruction("sub r10, r9");                                         // derive a candidate offset inside concat scratch
    emitter.instruction(&format!("cmp r10, {}", emitter.concat_capacity));          // determine whether the reservation is heap-backed
    emitter.instruction("jb __rt_str_bitwise_dest_ready_x86");                  // scratch-backed results have no heap header
    emitter.instruction(&format!("mov r9, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)));  // mark an unowned transient string result
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // let string persistence take over a heap fallback in place
    emitter.label("__rt_str_bitwise_dest_ready_x86");

    emitter.instruction("xor r9d, r9d");                                        // initialize the byte index
    emitter.label("__rt_str_bitwise_loop_x86");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 48]");                        // test whether every result byte has been produced
    emitter.instruction("jae __rt_str_bitwise_done_x86");                       // finish at the PHP-selected result length
    emitter.instruction("xor ecx, ecx");                                        // absent left tail bytes contribute zero to OR
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // check whether the left operand has this byte
    emitter.instruction("jae __rt_str_bitwise_left_ready_x86");                 // keep zero beyond the left operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left byte-string pointer
    emitter.instruction("movzx ecx, BYTE PTR [rax + r9]");                      // load the current left byte
    emitter.label("__rt_str_bitwise_left_ready_x86");
    emitter.instruction("xor edi, edi");                                        // absent right tail bytes contribute zero to OR
    emitter.instruction("cmp r9, QWORD PTR [rbp - 32]");                        // check whether the right operand has this byte
    emitter.instruction("jae __rt_str_bitwise_right_ready_x86");                // keep zero beyond the right operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the right byte-string pointer
    emitter.instruction("movzx edi, BYTE PTR [rax + r9]");                      // load the current right byte
    emitter.label("__rt_str_bitwise_right_ready_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the operation selector
    emitter.instruction("cmp r8, 0");                                           // operation 0 selects AND
    emitter.instruction("je __rt_str_bitwise_and_x86");                         // branch to byte-wise AND
    emitter.instruction("cmp r8, 1");                                           // operation 1 selects OR
    emitter.instruction("je __rt_str_bitwise_or_x86");                          // branch to byte-wise OR
    emitter.instruction("xor ecx, edi");                                        // operation 2 computes byte-wise XOR
    emitter.instruction("jmp __rt_str_bitwise_store_x86");                      // store the XOR result byte
    emitter.label("__rt_str_bitwise_and_x86");
    emitter.instruction("and ecx, edi");                                        // compute byte-wise AND
    emitter.instruction("jmp __rt_str_bitwise_store_x86");                      // store the AND result byte
    emitter.label("__rt_str_bitwise_or_x86");
    emitter.instruction("or ecx, edi");                                         // compute byte-wise OR, including the longer tail
    emitter.label("__rt_str_bitwise_store_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the result base pointer
    emitter.instruction("mov BYTE PTR [r10 + r9], cl");                         // write the current result byte
    emitter.instruction("add r9, 1");                                           // advance to the next byte
    emitter.instruction("jmp __rt_str_bitwise_loop_x86");                       // continue until the selected result length

    emitter.label("__rt_str_bitwise_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // return the result pointer in the string ABI
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // return the selected result length
    emitter.instruction("call __rt_concat_publish");                            // publish only scratch-backed bytes
    emitter.instruction("add rsp, 64");                                         // release helper-local storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP string pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies both target emitters select PHP's min/max result lengths and byte operations.
    #[test]
    fn test_emit_str_bitwise_covers_both_target_abis() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_str_bitwise(&mut arm);
        let arm = arm.output();
        assert!(arm.contains("csel x6, x7, x6, eq"));
        assert!(arm.contains("and w10, w10, w11"));
        assert!(arm.contains("orr w10, w10, w11"));
        assert!(arm.contains("eor w10, w10, w11"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_str_bitwise(&mut x86);
        let x86 = x86.output();
        assert!(x86.contains("cmovb r9, rsi"));
        assert!(x86.contains("and ecx, edi"));
        assert!(x86.contains("or ecx, edi"));
        assert!(x86.contains("xor ecx, edi"));
    }
}
