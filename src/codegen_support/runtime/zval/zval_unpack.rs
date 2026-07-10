//! Purpose:
//! Emits the `__rt_zval_unpack` runtime helper that converts a PHP `zval`
//! structure back into a boxed elephc `Mixed` cell.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`.
//!
//! Key details:
//! - Input: `x0` / `rax` = zval pointer.
//! - Output: `x0` / `rax` = boxed `Mixed` cell pointer (produced via
//!   `__rt_mixed_from_value` with the recovered `(tag, lo, hi)` triple).
//! - String zvals are copied into an owned elephc string via `__rt_str_persist`.
//! - Array zvals are rebuilt into elephc arrays by `__rt_zval_unpack_array`
//!   (wired per stage) so the resulting cell holds runtime-managed storage.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_unpack: convert a PHP zval back into a boxed elephc Mixed cell.
/// Input:  x0 / rax = zval pointer
/// Output: x0 / rax = boxed Mixed cell pointer
pub fn emit_zval_unpack(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_unpack_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_unpack ---");
    emitter.label_global("__rt_zval_unpack");

    // -- set up stack frame and save the zval pointer --
    emitter.instruction("sub sp, sp, #48");                                     // reserve the zval pointer slot plus frame records
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the zval pointer across helper calls
    emitter.instruction("ldrb w9, [x0, #8]");                                   // load the PHP IS_* type byte

    // -- dispatch on the PHP type byte --
    emitter.instruction("cmp w9, #1");                                          // IS_NULL
    emitter.instruction("b.eq __rt_zval_unpack_null");                          // unpack as a PHP null
    emitter.instruction("cmp w9, #2");                                          // IS_FALSE
    emitter.instruction("b.eq __rt_zval_unpack_false");                         // unpack as boolean false
    emitter.instruction("cmp w9, #3");                                          // IS_TRUE
    emitter.instruction("b.eq __rt_zval_unpack_true");                          // unpack as boolean true
    emitter.instruction("cmp w9, #4");                                          // IS_LONG
    emitter.instruction("b.eq __rt_zval_unpack_long");                          // unpack as a PHP integer
    emitter.instruction("cmp w9, #5");                                          // IS_DOUBLE
    emitter.instruction("b.eq __rt_zval_unpack_double");                        // unpack as a PHP float
    emitter.instruction("cmp w9, #6");                                          // IS_STRING
    emitter.instruction("b.eq __rt_zval_unpack_string");                        // unpack the zend_string into an elephc string
    emitter.instruction("cmp w9, #7");                                          // IS_ARRAY
    emitter.instruction("b.eq __rt_zval_unpack_is_array");                      // unpack the zend_array into an elephc array
    emitter.instruction("cmp w9, #8");                                          // IS_OBJECT
    emitter.instruction("b.eq __rt_zval_unpack_object");                        // unpack the object pointer
    emitter.instruction("cmp w9, #9");                                          // IS_RESOURCE
    emitter.instruction("b.eq __rt_zval_unpack_resource");                      // unpack the resource pointer
    emitter.instruction("b __rt_zval_unpack_null");                             // unknown kinds unpack as null

    // -- null: tag 8, zero payload --
    emitter.label("__rt_zval_unpack_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, xzr");                                         // lo = 0
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- false: tag 3, payload 0 --
    emitter.label("__rt_zval_unpack_false");
    emitter.instruction("mov x0, #3");                                          // tag = 3 (bool)
    emitter.instruction("mov x1, xzr");                                         // lo = 0 (false)
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- true: tag 3, payload 1 --
    emitter.label("__rt_zval_unpack_true");
    emitter.instruction("mov x0, #3");                                          // tag = 3 (bool)
    emitter.instruction("mov x1, #1");                                          // lo = 1 (true)
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- long: tag 0, payload = zval value --
    emitter.label("__rt_zval_unpack_long");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the zval pointer
    emitter.instruction("ldr x1, [x10]");                                       // lo = integer value
    emitter.instruction("mov x0, #0");                                          // tag = 0 (int)
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- double: tag 2, payload = f64 bits --
    emitter.label("__rt_zval_unpack_double");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the zval pointer
    emitter.instruction("ldr x1, [x10]");                                       // lo = float bits
    emitter.instruction("mov x0, #2");                                          // tag = 2 (float)
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- object: tag 6, payload = elephc object pointer --
    emitter.label("__rt_zval_unpack_object");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the zval pointer
    emitter.instruction("ldr x1, [x10]");                                       // lo = object pointer
    emitter.instruction("mov x0, #6");                                          // tag = 6 (object)
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- resource: tag 9, payload = elephc resource pointer --
    emitter.label("__rt_zval_unpack_resource");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the zval pointer
    emitter.instruction("ldr x1, [x10]");                                       // lo = resource pointer
    emitter.instruction("mov x0, #9");                                          // tag = 9 (resource)
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- string: copy zend_string bytes into an owned elephc string --
    emitter.label("__rt_zval_unpack_string");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the zval pointer
    emitter.instruction("ldr x10, [x10]");                                      // load the zend_string pointer
    emitter.instruction("ldr x2, [x10, #16]");                                  // load the zend_string length
    emitter.instruction("add x1, x10, #24");                                    // x1 = zend_string val[] base
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned elephc string pointer, x2 = length
    emitter.instruction("mov x0, #1");                                          // tag = 1 (string)
    emitter.instruction("b __rt_zval_unpack_build");                            // build the Mixed cell from the recovered triple

    // -- array: rebuild a fresh elephc array, then own-transfer box it into a Mixed cell --
    // The rebuilt array is freshly allocated (refcount 1), so the cell takes its
    // single ref directly instead of going through __rt_mixed_from_value (which
    // would incref the array and leak the alloc ref). The cell shape matches a
    // normal Mixed box produced by __rt_mixed_from_value.
    emitter.label("__rt_zval_unpack_is_array");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the zval pointer
    emitter.instruction("ldr x0, [x10]");                                       // load the zend_array pointer
    emitter.instruction("bl __rt_zval_unpack_array");                           // x0 = runtime tag (4 indexed, 5 hash), x1 = fresh array (refcount 1)
    emitter.instruction("str x0, [sp, #24]");                                   // save the runtime array tag across the cell alloc
    emitter.instruction("str x1, [sp, #16]");                                   // save the owned array pointer across the cell alloc
    emitter.instruction("mov x0, #24");                                         // a Mixed cell stores tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the Mixed cell storage
    emitter.instruction("mov x9, #5");                                          // low byte 5 = Mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the Mixed-cell heap kind in the uniform header
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the runtime array tag (4 or 5)
    emitter.instruction("str x9, [x0]");                                        // store the runtime value tag at mixed[0]
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the owned array pointer
    emitter.instruction("str x1, [x0, #8]");                                    // store the array pointer at mixed[8] (no incref: cell owns it)
    emitter.instruction("str xzr, [x0, #16]");                                  // hi = 0 (array payload is single-word)
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return the Mixed cell pointer in x0

    // -- build the mixed cell from the recovered (tag, lo, hi) triple --
    emitter.label("__rt_zval_unpack_build");
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed Mixed cell pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return the Mixed cell pointer in x0
}

/// x86_64 Linux implementation of `__rt_zval_unpack`.
/// Input:  rax = zval pointer
/// Output: rax = boxed Mixed cell pointer
fn emit_zval_unpack_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_unpack ---");
    emitter.label_global("__rt_zval_unpack");

    // -- set up stack frame and save the zval pointer --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve the zval pointer slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the zval pointer across helper calls
    emitter.instruction("movzx rcx, BYTE PTR [rax + 8]");                       // load the PHP IS_* type byte

    // -- dispatch on the PHP type byte --
    emitter.instruction("cmp rcx, 1");                                          // IS_NULL
    emitter.instruction("je __rt_zval_unpack_null");                            // unpack as a PHP null
    emitter.instruction("cmp rcx, 2");                                          // IS_FALSE
    emitter.instruction("je __rt_zval_unpack_false");                           // unpack as boolean false
    emitter.instruction("cmp rcx, 3");                                          // IS_TRUE
    emitter.instruction("je __rt_zval_unpack_true");                            // unpack as boolean true
    emitter.instruction("cmp rcx, 4");                                          // IS_LONG
    emitter.instruction("je __rt_zval_unpack_long");                            // unpack as a PHP integer
    emitter.instruction("cmp rcx, 5");                                          // IS_DOUBLE
    emitter.instruction("je __rt_zval_unpack_double");                          // unpack as a PHP float
    emitter.instruction("cmp rcx, 6");                                          // IS_STRING
    emitter.instruction("je __rt_zval_unpack_string");                          // unpack the zend_string into an elephc string
    emitter.instruction("cmp rcx, 7");                                          // IS_ARRAY
    emitter.instruction("je __rt_zval_unpack_is_array");                        // unpack the zend_array into an elephc array
    emitter.instruction("cmp rcx, 8");                                          // IS_OBJECT
    emitter.instruction("je __rt_zval_unpack_object");                          // unpack the object pointer
    emitter.instruction("cmp rcx, 9");                                          // IS_RESOURCE
    emitter.instruction("je __rt_zval_unpack_resource");                        // unpack the resource pointer
    emitter.instruction("jmp __rt_zval_unpack_null");                           // unknown kinds unpack as null

    // -- null: tag 8, zero payload --
    emitter.label("__rt_zval_unpack_null");
    emitter.instruction("mov eax, 8");                                          // tag = 8 (null)
    emitter.instruction("xor edi, edi");                                        // lo = 0
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- false: tag 3, payload 0 --
    emitter.label("__rt_zval_unpack_false");
    emitter.instruction("mov eax, 3");                                          // tag = 3 (bool)
    emitter.instruction("xor edi, edi");                                        // lo = 0 (false)
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- true: tag 3, payload 1 --
    emitter.label("__rt_zval_unpack_true");
    emitter.instruction("mov eax, 3");                                          // tag = 3 (bool)
    emitter.instruction("mov edi, 1");                                          // lo = 1 (true)
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- long: tag 0, payload = zval value --
    emitter.label("__rt_zval_unpack_long");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the zval pointer
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // lo = integer value
    emitter.instruction("xor eax, eax");                                        // tag = 0 (int)
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- double: tag 2, payload = f64 bits --
    emitter.label("__rt_zval_unpack_double");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the zval pointer
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // lo = float bits
    emitter.instruction("mov eax, 2");                                          // tag = 2 (float)
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- object: tag 6, payload = elephc object pointer --
    emitter.label("__rt_zval_unpack_object");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the zval pointer
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // lo = object pointer
    emitter.instruction("mov eax, 6");                                          // tag = 6 (object)
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- resource: tag 9, payload = elephc resource pointer --
    emitter.label("__rt_zval_unpack_resource");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the zval pointer
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // lo = resource pointer
    emitter.instruction("mov eax, 9");                                          // tag = 9 (resource)
    emitter.instruction("xor esi, esi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- string: copy zend_string bytes into an owned elephc string --
    emitter.label("__rt_zval_unpack_string");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the zval pointer
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the zend_string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // load the zend_string length
    emitter.instruction("lea rax, [r10 + 24]");                                 // rax = zend_string val[] base
    emitter.instruction("call __rt_str_persist");                               // rax = owned elephc string pointer, rdx = length
    emitter.instruction("mov rdi, rax");                                        // lo = owned string pointer
    emitter.instruction("mov rsi, rdx");                                        // hi = string length
    emitter.instruction("mov eax, 1");                                          // tag = 1 (string)
    emitter.instruction("jmp __rt_zval_unpack_build");                          // build the Mixed cell from the recovered triple

    // -- array: rebuild a fresh elephc array, then own-transfer box it into a Mixed cell --
    // The rebuilt array is freshly allocated (refcount 1), so the cell takes its
    // single ref directly (no incref) to avoid leaking the alloc ref through the
    // shared __rt_mixed_from_value retain path.
    emitter.label("__rt_zval_unpack_is_array");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the zval pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the zend_array pointer
    emitter.instruction("call __rt_zval_unpack_array");                         // rax = runtime tag (4 indexed, 5 hash), rdx = fresh array (refcount 1)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the runtime array tag across the cell alloc
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the owned array pointer across the cell alloc
    emitter.instruction("mov rax, 24");                                         // a Mixed cell stores tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the Mixed cell storage
    emitter.instruction("mov r10, 0x454C504800000005");                         // materialize the Mixed-cell heap kind word (x86_64 marker | kind 5)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a Mixed cell in the uniform header
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the runtime array tag (4 or 5)
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag at mixed[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the owned array pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the array pointer at mixed[8] (no incref: cell owns it)
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // hi = 0 (array payload is single-word)
    emitter.instruction("add rsp, 32");                                         // release the local slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the Mixed cell pointer in rax

    // -- build the mixed cell from the recovered (tag, lo, hi) triple --
    emitter.label("__rt_zval_unpack_build");
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed Mixed cell pointer
    emitter.instruction("add rsp, 32");                                         // release the local slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the Mixed cell pointer in rax
}
