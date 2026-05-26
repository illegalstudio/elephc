//! Purpose:
//! Emits the `__rt_hash_array_union` runtime helper for associative+indexed array union.
//! Appends only right indexed numeric keys that are absent from the cloned left hash.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - PHP array union uses normalized integer keys for indexed positions, so left hash keys like `0` or `"0"` block right index `0`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_array_union` runtime helper.
///
/// Performs PHP array union (`$left + $right`) where the left operand is an
/// associative hash and the right operand is an indexed array. The left hash
/// is cloned, then every right indexed entry whose normalized integer key is
/// absent from the cloned left is copied into the result. Duplicate keys retain
/// the left value and skip the right entry.
///
/// # Inputs
/// - `x0`: left hash pointer (preserved semantics; result hash is returned in `x0`)
/// - `x1`: right indexed-array pointer
///
/// # Output
/// - `x0`: result hash pointer (owned clone of left, augmented with missing right entries)
///
/// # Calling convention
/// Uses `__rt_hash_clone_shallow`, `__rt_hash_get`, `__rt_hash_set`,
/// `__rt_incref` (for refcounted payloads), and `__rt_str_persist` (for strings).
/// Clobbers `x0`–`x5`, `x9`–`x13` and caller-saved registers on ARM64; `rax`,
/// `rcx`, `rdx`, `rsi`, `rdi`, `r8`–`r12` on x86_64.
pub fn emit_hash_array_union(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_array_union_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_array_union ---");
    emitter.label_global("__rt_hash_array_union");

    // -- set up stack frame and clone the left hash --
    emitter.instruction("sub sp, sp, #96");                                     // reserve spill slots for hash-to-indexed union state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right indexed-array pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone the left hash so the union result owns its entries
    emitter.instruction("str x0, [sp, #8]");                                    // save the evolving result hash pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the right indexed-array pointer
    emitter.instruction("ldr x10, [x9]");                                       // load the right indexed-array length
    emitter.instruction("str x10, [sp, #24]");                                  // save the right indexed-array length for loop bounds
    emitter.instruction("ldr x11, [x9, #-8]");                                  // load the packed right indexed-array kind word
    emitter.instruction("lsr x11, x11, #8");                                    // move the right value_type tag into the low bits
    emitter.instruction("and x11, x11, #0x7f");                                 // isolate the right value_type tag without the persistent COW flag
    emitter.instruction("str x11, [sp, #32]");                                  // save the right value_type tag for copy dispatch
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the right numeric key cursor

    // -- append missing right indexed keys to the cloned left hash --
    emitter.label("__rt_hash_array_union_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the current right numeric key
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the right indexed-array length
    emitter.instruction("cmp x9, x10");                                         // have all right indexed keys been considered?
    emitter.instruction("b.ge __rt_hash_array_union_done");                     // finish once the right indexed array is exhausted
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the current result hash pointer
    emitter.instruction("mov x1, x9");                                          // use the right index as a normalized integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks the key as integer
    emitter.instruction("bl __rt_hash_get");                                    // test whether the left/result already contains this numeric key
    emitter.instruction("cbnz x0, __rt_hash_array_union_next");                 // duplicate keys keep the left value and skip insertion
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the current right numeric key after lookup clobbered registers
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the right value_type tag
    emitter.instruction("cmp x12, #1");                                         // is the right indexed array storing string slots?
    emitter.instruction("b.eq __rt_hash_array_union_string");                   // strings need persistence before hash insertion
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the right indexed-array pointer
    emitter.instruction("add x11, x11, #24");                                   // move to the right indexed-array payload base
    emitter.instruction("ldr x3, [x11, x9, lsl #3]");                           // load the scalar or pointer payload for this numeric key
    emitter.instruction("str x3, [sp, #40]");                                   // save the copied low payload word across optional retain calls
    emitter.instruction("cmp x12, #4");                                         // is the right payload in the refcounted range?
    emitter.instruction("b.lo __rt_hash_array_union_scalar");                   // scalar payloads can be copied directly
    emitter.instruction("cmp x12, #7");                                         // is the right payload still a supported refcounted value?
    emitter.instruction("b.hi __rt_hash_array_union_scalar");                   // unknown high tags fall back to scalar copying
    emitter.instruction("mov x0, x3");                                          // move the borrowed right heap payload into the retain helper
    emitter.instruction("bl __rt_incref");                                      // retain the right heap payload for the result hash owner
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the retained payload low word
    emitter.instruction("mov x4, xzr");                                         // refcounted values use only the low payload word
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload the runtime value tag
    emitter.instruction("b __rt_hash_array_union_insert");                      // insert the retained right payload

    emitter.label("__rt_hash_array_union_scalar");
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the scalar right payload low word
    emitter.instruction("mov x4, xzr");                                         // scalar copied values do not use a high payload word here
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload the scalar runtime value tag
    emitter.instruction("b __rt_hash_array_union_insert");                      // insert the scalar right payload

    emitter.label("__rt_hash_array_union_string");
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the right indexed-array pointer
    emitter.instruction("lsl x13, x9, #4");                                     // compute the byte offset for a 16-byte string slot
    emitter.instruction("add x11, x11, x13");                                   // advance to the selected string slot
    emitter.instruction("add x11, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("ldp x1, x2, [x11]");                                   // load the borrowed right string pointer and length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the result hash owner
    emitter.instruction("mov x3, x1");                                          // move the owned string pointer into the hash value low word
    emitter.instruction("mov x4, x2");                                          // move the owned string length into the hash value high word
    emitter.instruction("mov x5, #1");                                          // value_tag 1 = string

    emitter.label("__rt_hash_array_union_insert");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the current result hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // use the right index as a normalized integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks the key as integer
    emitter.instruction("bl __rt_hash_set");                                    // append the missing right indexed entry to the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the possibly grown result hash pointer

    emitter.label("__rt_hash_array_union_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the right numeric key cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next right indexed key
    emitter.instruction("str x9, [sp, #16]");                                   // save the advanced right cursor
    emitter.instruction("b __rt_hash_array_union_loop");                        // continue scanning right indexed entries

    emitter.label("__rt_hash_array_union_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the union spill slots
    emitter.instruction("ret");                                                 // return to generated code
}

/// x86_64/Linux implementation of `__rt_hash_array_union`.
///
/// Identical in behavior to the ARM64 path but emits x86_64 machine code
/// using the System V ABI callee/caller conventions. The function entry point
/// is still named `__rt_hash_array_union`; the platform dispatcher in the
/// public `emit_hash_array_union` routes here based on `Arch::X86_64`.
///
/// # Inputs
/// - `rsi`: right indexed-array pointer
/// - `rax` (on entry): left hash pointer (passes through to clone helper)
///
/// # Output
/// - `rax`: result hash pointer
///
/// # Clobbered registers
/// `rax`, `rcx`, `rdx`, `rsi`, `rdi`, `r8`–`r12`, `r15`; preserves `rbx`,
/// `rbp`, `r12`–`r14`.
fn emit_hash_array_union_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_array_union ---");
    emitter.label_global("__rt_hash_array_union");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving union spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for hash-to-indexed union state
    emitter.instruction("sub rsp, 64");                                         // reserve local storage while keeping nested calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right indexed-array pointer
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone the left hash so the union result owns its entries
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the evolving result hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the right indexed-array pointer
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the right indexed-array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the right indexed-array length for loop bounds
    emitter.instruction("mov r11, QWORD PTR [r10 - 8]");                        // load the packed right indexed-array kind word
    emitter.instruction("shr r11, 8");                                          // move the right value_type tag into the low bits
    emitter.instruction("and r11, 0x7f");                                       // isolate the right value_type tag without the persistent COW flag
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the right value_type tag for copy dispatch
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the right numeric key cursor

    emitter.label("__rt_hash_array_union_x86_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current right numeric key
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all right indexed keys been considered?
    emitter.instruction("jge __rt_hash_array_union_x86_done");                  // finish once the right indexed array is exhausted
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the current result hash pointer
    emitter.instruction("mov rsi, r10");                                        // use the right index as a normalized integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks the key as integer
    emitter.instruction("call __rt_hash_get");                                  // test whether the left/result already contains this numeric key
    emitter.instruction("test rax, rax");                                       // did the lookup find a left-side value?
    emitter.instruction("jnz __rt_hash_array_union_x86_next");                  // duplicate keys keep the left value and skip insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current right numeric key after lookup clobbered registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the right value_type tag
    emitter.instruction("cmp r11, 1");                                          // is the right indexed array storing string slots?
    emitter.instruction("je __rt_hash_array_union_x86_string");                 // strings need persistence before hash insertion
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right indexed-array pointer
    emitter.instruction("mov rcx, QWORD PTR [rax + 24 + r10 * 8]");             // load the scalar or pointer payload for this numeric key
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the copied low payload word across optional retain calls
    emitter.instruction("cmp r11, 4");                                          // is the right payload in the refcounted range?
    emitter.instruction("jb __rt_hash_array_union_x86_scalar");                 // scalar payloads can be copied directly
    emitter.instruction("cmp r11, 7");                                          // is the right payload still a supported refcounted value?
    emitter.instruction("ja __rt_hash_array_union_x86_scalar");                 // unknown high tags fall back to scalar copying
    emitter.instruction("mov rax, rcx");                                        // move the borrowed right heap payload into the retain helper
    emitter.instruction("call __rt_incref");                                    // retain the right heap payload for the result hash owner
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the retained payload low word
    emitter.instruction("xor r8d, r8d");                                        // refcounted values use only the low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the runtime value tag
    emitter.instruction("jmp __rt_hash_array_union_x86_insert");                // insert the retained right payload

    emitter.label("__rt_hash_array_union_x86_scalar");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the scalar right payload low word
    emitter.instruction("xor r8d, r8d");                                        // scalar copied values do not use a high payload word here
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the scalar runtime value tag
    emitter.instruction("jmp __rt_hash_array_union_x86_insert");                // insert the scalar right payload

    emitter.label("__rt_hash_array_union_x86_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current right numeric key
    emitter.instruction("shl r10, 4");                                          // compute the byte offset for a 16-byte string slot
    emitter.instruction("lea r11, [rax + r10 + 24]");                           // address the selected right string slot
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the borrowed right string pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the borrowed right string length
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload for the result hash owner
    emitter.instruction("mov rcx, rax");                                        // move the owned string pointer into the hash value low word
    emitter.instruction("mov r8, rdx");                                         // move the owned string length into the hash value high word
    emitter.instruction("mov r9, 1");                                           // value_tag 1 = string

    emitter.label("__rt_hash_array_union_x86_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the current result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // use the right index as a normalized integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks the key as integer
    emitter.instruction("call __rt_hash_set");                                  // append the missing right indexed entry to the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the possibly grown result hash pointer

    emitter.label("__rt_hash_array_union_x86_next");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance to the next right indexed key
    emitter.instruction("jmp __rt_hash_array_union_x86_loop");                  // continue scanning right indexed entries

    emitter.label("__rt_hash_array_union_x86_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed result hash pointer
    emitter.instruction("add rsp, 64");                                         // release the union spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
