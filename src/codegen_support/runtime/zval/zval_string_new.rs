//! Purpose:
//! Emits the `__rt_zval_string_new` runtime helper that builds a PHP
//! `zend_string` structure from an elephc string (pointer + length).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and directly from `__rt_zval_pack` for the string kind.
//!
//! Key details:
//! - `zend_string` layout: `refcount`(u32)@0, `gc.type_info`(u32)@4, `h`(u64)@8,
//!   `len`(u64)@16, `val[]`@24, plus a NUL terminator at `24 + len`.
//! - The bytes are copied from the elephc source so the new `zend_string` owns
//!   independent storage (the elephc source is not consumed).
//! - Register convention matches `__rt_str_persist` per target so callers can
//!   stage the pointer/length pair once.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_string_new: allocate a zend_string and copy bytes from an elephc string.
/// Input:  x1 = source string pointer, x2 = source length
/// Output: x0 = zend_string pointer (val bytes start at offset 24)
pub fn emit_zval_string_new(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_string_new_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_string_new ---");
    emitter.label_global("__rt_zval_string_new");

    // -- set up stack frame and spill the source pointer and length --
    emitter.instruction("sub sp, sp, #48");                                     // reserve source/length/zend_str slots plus frame records
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the source string pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the source string length

    // -- allocate the zend_string storage (24 header + len + 1 NUL, aligned to 8) --
    emitter.instruction("add x0, x2, #25");                                     // header(24) + length + NUL byte
    emitter.instruction("add x0, x0, #7");                                      // round up to an 8-byte multiple
    emitter.instruction("bic x0, x0, #7");                                      // align the allocation size down to 8 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = zend_string storage pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the zend_string pointer

    // -- fill the zend_string header --
    emitter.instruction("mov x9, #1");                                          // refcount starts at one owner
    emitter.instruction("str w9, [x0]");                                        // store refcount at offset 0 (32-bit)
    emitter.instruction("mov x9, #6");                                          // gc type_info = IS_STRING (6)
    emitter.instruction("str w9, [x0, #4]");                                    // store gc type_info at offset 4 (32-bit)
    emitter.instruction("str xzr, [x0, #8]");                                   // zero the hash slot h at offset 8
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the source length
    emitter.instruction("str x9, [x0, #16]");                                   // store len at offset 16

    // -- copy source bytes into zend_string val[] at offset 24 --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source string pointer
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the zend_string pointer
    emitter.instruction("add x0, x0, #24");                                     // dst = zend_string val[] base
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the byte count
    emitter.instruction("cbz x2, __rt_zval_string_new_done_copy");              // skip the copy loop for empty strings

    emitter.label("__rt_zval_string_new_copy_loop");
    emitter.instruction("ldrb w9, [x1]");                                       // read the next source byte
    emitter.instruction("strb w9, [x0]");                                       // write the byte into the zend_string val[]
    emitter.instruction("add x1, x1, #1");                                      // advance the source cursor
    emitter.instruction("add x0, x0, #1");                                      // advance the destination cursor
    emitter.instruction("subs x2, x2, #1");                                     // decrement the remaining byte count
    emitter.instruction("b.ne __rt_zval_string_new_copy_loop");                 // loop until every byte is copied

    emitter.label("__rt_zval_string_new_done_copy");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the zend_string pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the length for the NUL offset
    emitter.instruction("add x3, x0, x2");                                      // compute the NUL address base (zend_str + len)
    emitter.instruction("strb wzr, [x3, #24]");                                 // write the NUL terminator at zend_str + 24 + len
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return the zend_string pointer in x0
}

/// x86_64 Linux implementation of `__rt_zval_string_new`.
/// Input:  rax = source string pointer, rdx = source length
/// Output: rax = zend_string pointer (val bytes start at offset 24)
fn emit_zval_string_new_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_string_new ---");
    emitter.label_global("__rt_zval_string_new");

    // -- set up stack frame and spill the source pointer and length --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve source/length/zend_str slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length

    // -- allocate the zend_string storage (24 header + len + 1 NUL, aligned to 8) --
    emitter.instruction("lea rax, [rdx + 25]");                                 // header(24) + length + NUL byte
    emitter.instruction("add rax, 7");                                          // round up to an 8-byte multiple
    emitter.instruction("and rax, -8");                                         // align the allocation size down to 8 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = zend_string storage pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the zend_string pointer

    // -- fill the zend_string header --
    emitter.instruction("mov DWORD PTR [rax], 1");                              // refcount starts at one owner (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 4], 6");                          // gc type_info = IS_STRING (6)
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // zero the hash slot h at offset 8
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length
    emitter.instruction("mov QWORD PTR [rax + 16], rdx");                       // store len at offset 16

    // -- copy source bytes into zend_string val[] at offset 24 --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source string pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the zend_string pointer
    emitter.instruction("add rdi, 24");                                         // dst = zend_string val[] base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the byte count
    emitter.instruction("test rcx, rcx");                                       // check for an empty string
    emitter.instruction("je __rt_zval_string_new_done_copy");                   // skip the copy loop for empty strings

    emitter.label("__rt_zval_string_new_copy_loop");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // read the next source byte
    emitter.instruction("mov BYTE PTR [rdi], al");                              // write the byte into the zend_string val[]
    emitter.instruction("inc rsi");                                             // advance the source cursor
    emitter.instruction("inc rdi");                                             // advance the destination cursor
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count
    emitter.instruction("jne __rt_zval_string_new_copy_loop");                  // loop until every byte is copied

    emitter.label("__rt_zval_string_new_done_copy");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the zend_string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length for the NUL offset
    emitter.instruction("mov byte ptr [rax + rcx + 24], 0");                    // write the NUL terminator at zend_str + 24 + len
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the zend_string pointer in rax
    emitter.instruction("add rsp, 32");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the zend_string pointer in rax
}
