//! Purpose:
//! Emits runtime dispatch for PHP bitwise operations on boxed gradual values.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the array runtime.
//!
//! Key details:
//! - Two runtime strings use byte-wise string semantics; every other scalar pair is cast to int.
//! - Results are returned as an owned boxed Mixed cell on both supported instruction sets.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits boxed runtime dispatch for PHP `&`, `|`, and `^`.
pub fn emit_mixed_bitwise(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_bitwise_x86_64(emitter);
    } else {
        emit_mixed_bitwise_aarch64(emitter);
    }
}

/// Emits the AArch64 boxed bitwise entry points and shared dispatcher.
fn emit_mixed_bitwise_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed bitwise dispatch ---");
    emit_aarch64_entry(emitter, "__rt_mixed_bitwise_and", 0);
    emit_aarch64_entry(emitter, "__rt_mixed_bitwise_or", 1);
    emit_aarch64_entry(emitter, "__rt_mixed_bitwise_xor", 2);

    emitter.label_shared("__rt_mixed_bitwise_common");
    emitter.instruction("str x0, [sp]");                                       // preserve the boxed left operand
    emitter.instruction("str x1, [sp, #8]");                                   // preserve the boxed right operand
    emitter.instruction("str x9, [sp, #16]");                                  // preserve the byte-wise operation selector
    emitter.instruction("bl __rt_mixed_unbox");                                // inspect the left runtime tag and payload words
    emitter.instruction("str x0, [sp, #24]");                                  // preserve the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #32]");                              // preserve the left payload words
    emitter.instruction("ldr x0, [sp, #8]");                                   // reload the boxed right operand
    emitter.instruction("bl __rt_mixed_unbox");                                // inspect the right runtime tag and payload words
    emitter.instruction("str x0, [sp, #48]");                                  // preserve the right runtime tag
    emitter.instruction("stp x1, x2, [sp, #56]");                              // preserve the right payload words
    emitter.instruction("ldr x9, [sp, #24]");                                  // reload the left runtime tag
    emitter.instruction("cmp x9, #1");                                         // runtime tag 1 denotes a PHP string
    emitter.instruction("b.ne __rt_mixed_bitwise_int");                        // a non-string operand selects integer coercion
    emitter.instruction("cmp x0, #1");                                         // check whether the right operand is also a string
    emitter.instruction("b.eq __rt_mixed_bitwise_string");                     // only two strings use byte-wise string semantics

    emitter.label("__rt_mixed_bitwise_int");
    emitter.instruction("ldr x0, [sp]");                                       // reload the boxed left operand for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                             // cast the left scalar through PHP integer rules
    emitter.instruction("str x0, [sp, #72]");                                  // preserve the left integer across the right cast
    emitter.instruction("ldr x0, [sp, #8]");                                   // reload the boxed right operand for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                             // cast the right scalar through PHP integer rules
    emitter.instruction("ldr x1, [sp, #72]");                                  // reload the left integer operand
    emitter.instruction("ldr x9, [sp, #16]");                                  // reload the operation selector
    emitter.instruction("cmp x9, #0");                                         // selector zero denotes bitwise AND
    emitter.instruction("b.eq __rt_mixed_bitwise_int_and");                    // branch to the integer AND instruction
    emitter.instruction("cmp x9, #1");                                         // selector one denotes bitwise OR
    emitter.instruction("b.eq __rt_mixed_bitwise_int_or");                     // branch to the integer OR instruction
    emitter.instruction("eor x1, x1, x0");                                     // selector two computes integer XOR
    emitter.instruction("b __rt_mixed_bitwise_box_int");                       // box the integer XOR result
    emitter.label("__rt_mixed_bitwise_int_and");
    emitter.instruction("and x1, x1, x0");                                     // compute integer AND
    emitter.instruction("b __rt_mixed_bitwise_box_int");                       // box the integer AND result
    emitter.label("__rt_mixed_bitwise_int_or");
    emitter.instruction("orr x1, x1, x0");                                     // compute integer OR
    emitter.label("__rt_mixed_bitwise_box_int");
    emitter.instruction("mov x2, xzr");                                        // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                         // runtime tag zero denotes an integer
    emitter.instruction("bl __rt_mixed_from_value");                           // allocate the boxed integer result
    emitter.instruction("b __rt_mixed_bitwise_done");                          // skip the string result path

    emitter.label("__rt_mixed_bitwise_string");
    emitter.instruction("ldp x1, x2, [sp, #32]");                              // load the left string pointer and length
    emitter.instruction("ldp x3, x4, [sp, #56]");                              // load the right string pointer and length
    emitter.instruction("ldr x5, [sp, #16]");                                  // pass the byte-wise operation selector
    emitter.instruction("bl __rt_str_bitwise");                                // compute a transient byte-string result
    emitter.instruction("bl __rt_str_persist");                                // turn scratch storage into an owned string
    emitter.instruction("stp x1, x2, [sp, #32]");                              // preserve the owned string while allocating its box
    emitter.instruction("mov x0, #24");                                        // mixed cells store a tag and two payload words
    emitter.instruction("bl __rt_heap_alloc");                                 // allocate the result Mixed cell
    emitter.instruction("mov x9, #5");                                         // heap kind five denotes a boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                  // stamp the uniform heap header
    emitter.instruction("mov x9, #1");                                         // runtime tag one denotes a string
    emitter.instruction("str x9, [x0]");                                       // store the string runtime tag
    emitter.instruction("ldp x1, x2, [sp, #32]");                              // reload the transferred owned string payload
    emitter.instruction("stp x1, x2, [x0, #8]");                               // move the string pointer and length into the box

    emitter.label("__rt_mixed_bitwise_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                            // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                   // release the shared dispatcher frame
    emitter.instruction("ret");                                                // return the owned Mixed result
}

/// Emits one AArch64 entry point with a fixed byte-wise selector.
fn emit_aarch64_entry(emitter: &mut Emitter, label: &str, selector: i64) {
    emitter.label_global(label);
    emitter.instruction("sub sp, sp, #112");                                   // reserve boxed operands, unboxed payloads, and the saved frame
    emitter.instruction("stp x29, x30, [sp, #96]");                            // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                   // establish a stable helper frame
    abi::emit_load_int_immediate(emitter, "x9", selector);
    emitter.instruction("b __rt_mixed_bitwise_common");                        // enter the shared runtime dispatcher
}

/// Emits the x86_64 boxed bitwise entry points and shared dispatcher.
fn emit_mixed_bitwise_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed bitwise dispatch ---");
    emit_x86_64_entry(emitter, "__rt_mixed_bitwise_and", 0);
    emit_x86_64_entry(emitter, "__rt_mixed_bitwise_or", 1);
    emit_x86_64_entry(emitter, "__rt_mixed_bitwise_xor", 2);

    emitter.label("__rt_mixed_bitwise_common_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // preserve the boxed left operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                      // preserve the boxed right operand
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                      // preserve the byte-wise operation selector
    emitter.instruction("call __rt_mixed_unbox");                              // inspect the left runtime tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                      // preserve the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                      // preserve the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                      // preserve the left high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                      // reload the boxed right operand
    emitter.instruction("call __rt_mixed_unbox");                              // inspect the right runtime tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                      // preserve the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                      // preserve the right low payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                      // preserve the right high payload word
    emitter.instruction("cmp QWORD PTR [rbp - 32], 1");                       // runtime tag one denotes a PHP string
    emitter.instruction("jne __rt_mixed_bitwise_int_x86_64");                  // a non-string operand selects integer coercion
    emitter.instruction("cmp rax, 1");                                         // check whether the right operand is also a string
    emitter.instruction("je __rt_mixed_bitwise_string_x86_64");                // only two strings use byte-wise string semantics

    emitter.label("__rt_mixed_bitwise_int_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // reload the boxed left operand for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                           // cast the left scalar through PHP integer rules
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                      // preserve the left integer across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                      // reload the boxed right operand for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                           // cast the right scalar through PHP integer rules
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                      // reload the left integer operand
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                       // selector zero denotes bitwise AND
    emitter.instruction("je __rt_mixed_bitwise_int_and_x86_64");               // branch to the integer AND instruction
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                       // selector one denotes bitwise OR
    emitter.instruction("je __rt_mixed_bitwise_int_or_x86_64");                // branch to the integer OR instruction
    emitter.instruction("xor rdi, rax");                                       // selector two computes integer XOR
    emitter.instruction("jmp __rt_mixed_bitwise_box_int_x86_64");              // box the integer XOR result
    emitter.label("__rt_mixed_bitwise_int_and_x86_64");
    emitter.instruction("and rdi, rax");                                       // compute integer AND
    emitter.instruction("jmp __rt_mixed_bitwise_box_int_x86_64");              // box the integer AND result
    emitter.label("__rt_mixed_bitwise_int_or_x86_64");
    emitter.instruction("or rdi, rax");                                        // compute integer OR
    emitter.label("__rt_mixed_bitwise_box_int_x86_64");
    emitter.instruction("xor rsi, rsi");                                       // integer payloads do not use a high word
    emitter.instruction("xor eax, eax");                                       // runtime tag zero denotes an integer
    emitter.instruction("call __rt_mixed_from_value");                         // allocate the boxed integer result
    emitter.instruction("jmp __rt_mixed_bitwise_done_x86_64");                 // skip the string result path

    emitter.label("__rt_mixed_bitwise_string_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                      // load the left string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                      // load the left string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                      // load the right string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                      // load the right string length
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                       // pass the byte-wise operation selector
    emitter.instruction("call __rt_str_bitwise");                              // compute a transient byte-string result
    emitter.instruction("call __rt_str_persist");                              // turn scratch storage into an owned string
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                      // preserve the owned string pointer while allocating its box
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                      // preserve the owned string length while allocating its box
    emitter.instruction("mov rax, 24");                                        // mixed cells store a tag and two payload words
    emitter.instruction("call __rt_heap_alloc");                               // allocate the result Mixed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the Mixed heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                       // stamp the uniform heap header
    emitter.instruction("mov QWORD PTR [rax], 1");                             // store runtime tag one for a string
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                      // reload the transferred owned string pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                       // move the string pointer into the box
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                      // reload the transferred owned string length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                      // move the string length into the box

    emitter.label("__rt_mixed_bitwise_done_x86_64");
    emitter.instruction("leave");                                              // release the helper frame and restore the caller frame pointer
    emitter.instruction("ret");                                                // return the owned Mixed result
}

/// Emits one x86_64 entry point with a fixed byte-wise selector.
fn emit_x86_64_entry(emitter: &mut Emitter, label: &str, selector: i64) {
    emitter.label_global(label);
    emitter.instruction("push rbp");                                           // preserve the caller frame pointer before nested helper calls
    emitter.instruction("mov rbp, rsp");                                       // establish a stable helper frame
    emitter.instruction("sub rsp, 96");                                        // reserve boxed operands, unboxed payloads, and scratch words
    abi::emit_load_int_immediate(emitter, "r10", selector);
    emitter.instruction("jmp __rt_mixed_bitwise_common_x86_64");               // enter the shared runtime dispatcher
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies that both target emitters contain string and integer dispatch paths.
    #[test]
    fn test_emit_mixed_bitwise_covers_both_target_abis() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_mixed_bitwise(&mut arm);
        let arm = arm.output();
        assert!(arm.contains("__rt_mixed_bitwise_string"));
        assert!(arm.contains("and x1, x1, x0"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_mixed_bitwise(&mut x86);
        let x86 = x86.output();
        assert!(x86.contains("__rt_mixed_bitwise_string_x86_64"));
        assert!(x86.contains("and rdi, rax"));
    }
}
