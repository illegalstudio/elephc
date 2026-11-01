//! Purpose:
//! Emits the `__rt_mixed_to_owned_hash` runtime helper for the gradual-typing boundary.
//! Converts a boxed Mixed/union array into a freshly owned hash without mutating the source.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays::emit_mixed_to_owned_hash()`.
//!
//! Key details:
//! - Tag 5 (associative): shallow-clones the payload hash and normalizes every value to a boxed
//!   Mixed cell, matching the EIR `AssocArray<Mixed, Mixed>` result contract.
//! - Tag 4 (indexed): rebuilds a hash, boxing each element via `__rt_mixed_array_get`.
//! - Nested Mixed wrappers are transparent: tag 7 cells are peeled before selecting an array
//!   representation, matching other gradual array consumers.
//! - Null/non-array payloads terminate with the shared gradual array-boundary `TypeError`.
//! - The returned hash is independently owned (refcount 1), so a single release frees it.

use crate::codegen::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;

/// Length in bytes of the shared `_array_arg_type_error_msg` fatal string.
const ARRAY_ARG_TYPE_ERROR_MSG_LEN: usize = 78;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies both target emitters peel nested Mixed cells before array dispatch.
    #[test]
    fn test_mixed_to_owned_hash_peels_nested_mixed_wrappers_on_all_targets() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_mixed_to_owned_hash(&mut arm);
        let arm = arm.output();
        assert!(arm.contains("    cmp x9, #7\n"));
        assert!(arm.contains("    b __rt_mixed_to_owned_hash_unwrap\n"));
        assert!(arm.contains("__rt_mixed_to_owned_hash_dispatch:\n"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_mixed_to_owned_hash(&mut x86);
        let x86 = x86.output();
        assert!(x86.contains("    cmp r10, 7\n"));
        assert!(x86.contains("    jmp __rt_mixed_to_owned_hash_unwrap\n"));
        assert!(x86.contains("__rt_mixed_to_owned_hash_dispatch:\n"));
    }
}

/// Emits the `__rt_mixed_to_owned_hash` runtime helper. Dispatches per target.
pub fn emit_mixed_to_owned_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_to_owned_hash_x86_64(emitter);
        return;
    }
    emit_mixed_to_owned_hash_aarch64(emitter);
}

/// Emits `__rt_mixed_to_owned_hash` for ARM64.
///
/// Input: `x0` = boxed Mixed array pointer. Output: `x0` = freshly owned hash (refcount 1).
fn emit_mixed_to_owned_hash_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_to_owned_hash ---");
    emitter.label_global("__rt_mixed_to_owned_hash");

    // Stack: [sp,#0]=cell, [sp,#8]=dst, [sp,#16]=i, [sp,#24]=len, [sp,#32]=x29/x30.
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame for the conversion state
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the local frame
    emitter.instruction("cbz x0, __rt_mixed_to_owned_hash_bad");                // null is not a valid gradual array operand
    emitter.label("__rt_mixed_to_owned_hash_unwrap");
    emitter.instruction("ldr x9, [x0]");                                        // load the Mixed runtime tag
    emitter.instruction("cmp x9, #7");                                          // does this box wrap another Mixed cell?
    emitter.instruction("b.ne __rt_mixed_to_owned_hash_dispatch");              // dispatch once the concrete payload tag is reached
    emitter.instruction("ldr x0, [x0, #8]");                                    // follow the nested boxed value
    emitter.instruction("cbz x0, __rt_mixed_to_owned_hash_bad");                // a null nested cell cannot represent an array
    emitter.instruction("b __rt_mixed_to_owned_hash_unwrap");                   // peel arbitrarily nested Mixed wrappers
    emitter.label("__rt_mixed_to_owned_hash_dispatch");
    emitter.instruction("str x0, [sp, #0]");                                    // retain the canonical boxed array cell for indexed element reads
    emitter.instruction("cmp x9, #5");                                          // tag 5 = associative array?
    emitter.instruction("b.eq __rt_mixed_to_owned_hash_assoc");                 // shallow-clone the hash payload
    emitter.instruction("cmp x9, #4");                                          // tag 4 = indexed array?
    emitter.instruction("b.eq __rt_mixed_to_owned_hash_indexed");               // rebuild a hash from the indexed array
    emitter.instruction("b __rt_mixed_to_owned_hash_bad");                      // reject every non-array payload at the gradual boundary

    // -- associative: clone the payload hash for an independent owned copy --
    emitter.label("__rt_mixed_to_owned_hash_assoc");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the hash payload pointer
    emitter.instruction("cbz x0, __rt_mixed_to_owned_hash_empty");              // defensive null guard
    emitter.instruction("bl __rt_hash_clone_shallow");                          // shallow-clone the hash into an owned refcount-1 copy
    emitter.instruction("bl __rt_hash_to_mixed");                              // normalize typed source payloads to the promised boxed-Mixed layout
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return the cloned hash

    // -- indexed: rebuild a hash, boxing each element as a Mixed value --
    emitter.label("__rt_mixed_to_owned_hash_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the indexed-array payload pointer
    emitter.instruction("cbz x10, __rt_mixed_to_owned_hash_empty");             // defensive null guard
    emitter.instruction("ldr x11, [x10]");                                      // load the array length
    emitter.instruction("str x11, [sp, #24]");                                  // save the length for the build loop
    emitter.instruction("mov x0, x11");                                         // capacity = length
    emitter.instruction("cmp x11, #0");                                         // is the indexed array empty?
    emitter.instruction("b.ne __rt_mixed_to_owned_hash_alloc");                 // non-empty arrays size the hash to their length
    emitter.instruction("mov x0, #1");                                          // empty arrays still need a non-zero hash capacity
    emitter.label("__rt_mixed_to_owned_hash_alloc");
    emitter.instruction("mov x1, #7");                                          // table value_type 7 = boxed Mixed entries
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the destination hash pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the element index to 0
    emitter.label("__rt_mixed_to_owned_hash_loop");
    emitter.instruction("ldr x12, [sp, #16]");                                  // load the current index
    emitter.instruction("ldr x13, [sp, #24]");                                  // load the length
    emitter.instruction("cmp x12, x13");                                        // have all elements been copied?
    emitter.instruction("b.ge __rt_mixed_to_owned_hash_done");                  // finish once every element is inserted
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed Mixed receiver
    emitter.instruction("mov x1, x12");                                         // element index as the lookup key
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("bl __rt_mixed_array_get");                             // read and box the element as an owned Mixed cell
    emitter.instruction("mov x3, x0");                                          // boxed element becomes the inserted value_lo
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the destination hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // integer key = element index
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov x4, #0");                                          // boxed Mixed values use no high word
    emitter.instruction("mov x5, #7");                                          // value_type 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_insert_owned");                           // insert the owned boxed element, transferring ownership
    emitter.instruction("str x0, [sp, #8]");                                    // save the possibly-grown destination hash pointer
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload the current index
    emitter.instruction("add x12, x12, #1");                                    // advance to the next element
    emitter.instruction("str x12, [sp, #16]");                                  // save the advanced index
    emitter.instruction("b __rt_mixed_to_owned_hash_loop");                     // continue inserting elements
    emitter.label("__rt_mixed_to_owned_hash_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the rebuilt hash
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return the rebuilt hash

    // -- defensive null payload: return a fresh empty hash --
    emitter.label("__rt_mixed_to_owned_hash_empty");
    emitter.instruction("mov x0, #1");                                          // minimum non-zero hash capacity
    emitter.instruction("mov x1, #7");                                          // table value_type 7 = boxed Mixed entries
    emitter.instruction("bl __rt_hash_new");                                    // allocate an empty owned hash
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return the empty hash

    // -- non-array payload: emit the TypeError and terminate the process --
    emitter.label("__rt_mixed_to_owned_hash_bad");
    abi::emit_symbol_address(emitter, "x1", "_array_arg_type_error_msg");
    emitter.instruction(&format!("mov x2, #{}", ARRAY_ARG_TYPE_ERROR_MSG_LEN)); // pass the gradual array fatal message length
    emitter.instruction("mov x0, #2");                                          // write the gradual array diagnostic to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE after the fatal type violation
    emitter.syscall(1);
}

/// Emits `__rt_mixed_to_owned_hash` for x86_64.
///
/// Input: `rdi` = boxed Mixed array pointer. Output: `rax` = freshly owned hash (refcount 1).
fn emit_mixed_to_owned_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_to_owned_hash ---");
    emitter.label_global("__rt_mixed_to_owned_hash");

    // Stack: [rbp-8]=cell, [rbp-16]=dst, [rbp-24]=i, [rbp-32]=len.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve aligned local storage for the conversion state
    emitter.instruction("test rdi, rdi");                                       // null receiver → fresh empty hash
    emitter.instruction("je __rt_mixed_to_owned_hash_bad");                     // null is not a valid gradual array operand
    emitter.label("__rt_mixed_to_owned_hash_unwrap");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the Mixed runtime tag
    emitter.instruction("cmp r10, 7");                                          // does this box wrap another Mixed cell?
    emitter.instruction("jne __rt_mixed_to_owned_hash_dispatch");              // dispatch once the concrete payload tag is reached
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // follow the nested boxed value
    emitter.instruction("test rdi, rdi");                                       // a null nested cell cannot represent an array
    emitter.instruction("je __rt_mixed_to_owned_hash_bad");
    emitter.instruction("jmp __rt_mixed_to_owned_hash_unwrap");                 // peel arbitrarily nested Mixed wrappers
    emitter.label("__rt_mixed_to_owned_hash_dispatch");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the canonical boxed array cell for indexed element reads
    emitter.instruction("cmp r10, 5");                                          // tag 5 = associative array?
    emitter.instruction("je __rt_mixed_to_owned_hash_assoc");                   // shallow-clone the hash payload
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array?
    emitter.instruction("je __rt_mixed_to_owned_hash_indexed");                 // rebuild a hash from the indexed array
    emitter.instruction("jmp __rt_mixed_to_owned_hash_bad");                    // reject every non-array payload at the gradual boundary

    emitter.label("__rt_mixed_to_owned_hash_assoc");
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the hash payload pointer
    emitter.instruction("test rdi, rdi");                                       // defensive null guard
    emitter.instruction("je __rt_mixed_to_owned_hash_empty");                   // branch to the empty-hash path on null
    emitter.instruction("call __rt_hash_clone_shallow");                        // shallow-clone the hash into an owned refcount-1 copy
    emitter.instruction("mov rdi, rax");                                       // pass the owned clone to the boxed-Mixed value normalizer
    emitter.instruction("call __rt_hash_to_mixed");                            // normalize typed source payloads to the promised boxed-Mixed layout
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the cloned hash

    emitter.label("__rt_mixed_to_owned_hash_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the indexed-array payload pointer
    emitter.instruction("test r10, r10");                                       // defensive null guard
    emitter.instruction("je __rt_mixed_to_owned_hash_empty");                   // branch to the empty-hash path on null
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the length for the build loop
    emitter.instruction("mov rdi, r11");                                        // capacity = length
    emitter.instruction("test r11, r11");                                       // is the indexed array empty?
    emitter.instruction("jne __rt_mixed_to_owned_hash_alloc");                  // non-empty arrays size the hash to their length
    emitter.instruction("mov rdi, 1");                                          // empty arrays still need a non-zero hash capacity
    emitter.label("__rt_mixed_to_owned_hash_alloc");
    emitter.instruction("mov rsi, 7");                                          // table value_type 7 = boxed Mixed entries
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the destination hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the element index to 0
    emitter.label("__rt_mixed_to_owned_hash_loop");
    emitter.instruction("mov r12, QWORD PTR [rbp - 24]");                       // load the current index
    emitter.instruction("cmp r12, QWORD PTR [rbp - 32]");                       // have all elements been copied?
    emitter.instruction("jge __rt_mixed_to_owned_hash_done");                   // finish once every element is inserted
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed Mixed receiver
    emitter.instruction("mov rsi, r12");                                        // element index as the lookup key
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("call __rt_mixed_array_get");                           // read and box the element as an owned Mixed cell
    emitter.instruction("mov rcx, rax");                                        // boxed element becomes the inserted value_lo
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // integer key = element index
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed values use no high word
    emitter.instruction("mov r9, 7");                                           // value_type 7 = boxed Mixed
    emitter.instruction("call __rt_hash_insert_owned");                         // insert the owned boxed element, transferring ownership
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the possibly-grown destination hash pointer
    emitter.instruction("mov r12, QWORD PTR [rbp - 24]");                       // reload the current index
    emitter.instruction("add r12, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 24], r12");                       // save the advanced index
    emitter.instruction("jmp __rt_mixed_to_owned_hash_loop");                   // continue inserting elements
    emitter.label("__rt_mixed_to_owned_hash_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the rebuilt hash
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rebuilt hash

    emitter.label("__rt_mixed_to_owned_hash_empty");
    emitter.instruction("mov rdi, 1");                                          // minimum non-zero hash capacity
    emitter.instruction("mov rsi, 7");                                          // table value_type 7 = boxed Mixed entries
    emitter.instruction("call __rt_hash_new");                                  // allocate an empty owned hash
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty hash

    // -- non-array payload: emit the TypeError and terminate the process --
    emitter.label("__rt_mixed_to_owned_hash_bad");
    emitter.instruction("mov edi, 2");                                          // write the gradual array diagnostic to stderr
    abi::emit_symbol_address(emitter, "rsi", "_array_arg_type_error_msg");
    emitter.instruction(&format!("mov edx, {}", ARRAY_ARG_TYPE_ERROR_MSG_LEN)); // pass the gradual array fatal message length
    emitter.instruction("mov eax, 1");                                          // select the Linux write syscall
    emitter.instruction("syscall");                                             // emit the gradual array fatal diagnostic
    emitter.instruction("mov edi, 70");                                         // use EX_SOFTWARE after the fatal type violation
    emitter.instruction("mov eax, 60");                                         // select the Linux exit syscall
    emitter.instruction("syscall");                                             // terminate after the gradual array type error
}
