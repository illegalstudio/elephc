//! Purpose:
//! Emits the `__rt_array_key_exists_mixed_key` runtime helper: presence-only
//! `array_key_exists()` for a statically `Array(_)` indexed local whose key is
//! a boxed `Mixed` cell or a string — the presence-only sibling of
//! `__rt_array_get_mixed_key`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The key tag is only known at runtime. The helper tag-dispatches on the
//!   array's storage kind exactly like `__rt_array_get_mixed_key`: kind 2
//!   (packed/indexed) storage never holds a string key, so a string key there
//!   is always absent; kind 3 (hash) storage delegates straight to
//!   `__rt_hash_get`'s found flag.
//! - Unlike `__rt_array_get_mixed_key`, this never materializes, boxes, or
//!   retains a value and never warns — `array_key_exists()` is presence-only
//!   and silent, and critically must answer `true` for a key whose stored
//!   value is null (the exact case `isset()` must answer `false` for), so it
//!   cannot be built by reusing the read helper plus an is-null check.
//! - Inputs are array pointer and normalized key pair. The result is a plain
//!   0/1 found flag in x0 (AArch64) / rax (x86_64).

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the mixed-key indexed/hash array presence probe for the current target.
pub fn emit_array_key_exists_mixed_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_key_exists_mixed_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_key_exists_mixed_key ---");
    emitter.label_global("__rt_array_key_exists_mixed_key");

    // Stack:
    //   [sp, #0]  = array_ptr
    //   [sp, #8]  = key_lo
    //   [sp, #16] = key_hi
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame: 3 inputs + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the key low word
    emitter.instruction("str x2, [sp, #16]");                                   // save the key high word (sentinel)

    emitter.instruction("cbz x0, __rt_array_key_exists_mixed_key_not_found");   // null array → not found

    // -- dispatch on array storage kind --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed kind metadata from the array header
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte (kind tag)
    emitter.instruction("cmp x9, #3");                                          // kind 3 = hash storage?
    emitter.instruction("b.eq __rt_array_key_exists_mixed_key_hash");           // route hash-storage arrays through hash_get's found flag
    emitter.instruction("cmp x9, #2");                                          // kind 2 = indexed storage?
    emitter.instruction("b.ne __rt_array_key_exists_mixed_key_not_found");      // unknown kind → not found

    // -- indexed storage: dispatch on key tag --
    emitter.label("__rt_array_key_exists_mixed_key_indexed");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload key_hi
    emitter.instruction("cmn x11, #1");                                         // compare with -1 (int-key sentinel)
    emitter.instruction("b.ne __rt_array_key_exists_mixed_key_not_found");      // a string key never exists on packed/indexed storage

    // -- integer key on indexed storage: bounds-check only --
    emitter.instruction("ldr x12, [sp, #8]");                                   // x12 = key_lo (int index)
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length (header offset 0)
    emitter.instruction("cmp x12, #0");                                         // negative index → not found
    emitter.instruction("b.lt __rt_array_key_exists_mixed_key_not_found");      // negative indexed-array keys never exist
    emitter.instruction("cmp x12, x9");                                         // index >= length → not found
    emitter.instruction("b.ge __rt_array_key_exists_mixed_key_not_found");      // out-of-bounds indexed-array keys never exist
    emitter.instruction("mov x0, #1");                                          // in-bounds integer key → found
    emitter.instruction("b __rt_array_key_exists_mixed_key_done");              // skip the hash and not-found arms

    // -- hash storage: delegate to __rt_hash_get's found flag ---
    emitter.label("__rt_array_key_exists_mixed_key_hash");
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = key_hi
    emitter.instruction("bl __rt_hash_get");                                    // x0 = found (a null-valued entry still reports found)
    emitter.instruction("b __rt_array_key_exists_mixed_key_done");              // hash_get's found flag is already the result

    // -- not found --
    emitter.label("__rt_array_key_exists_mixed_key_not_found");
    emitter.instruction("mov x0, #0");                                          // x0 = not found

    emitter.label("__rt_array_key_exists_mixed_key_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return found flag in x0
}

/// Emits the x86_64 variant of `__rt_array_key_exists_mixed_key`.
fn emit_array_key_exists_mixed_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_key_exists_mixed_key ---");
    emitter.label_global("__rt_array_key_exists_mixed_key");

    // Stack layout (16-byte aligned):
    //   [rbp - 8]  = array_ptr
    //   [rbp - 16] = key_lo
    //   [rbp - 24] = key_hi
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve 32 bytes for locals (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the key low word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the key high word (sentinel)

    emitter.instruction("test rdi, rdi");                                       // null array check
    emitter.instruction("je __rt_array_key_exists_mixed_key_not_found");        // null array → not found

    // -- dispatch on array storage kind --
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load packed kind metadata from the array header
    emitter.instruction("and r9, 0xff");                                        // isolate the low byte (kind tag)
    emitter.instruction("cmp r9, 3");                                           // kind 3 = hash storage?
    emitter.instruction("je __rt_array_key_exists_mixed_key_hash");             // route hash-storage arrays through hash_get's found flag
    emitter.instruction("cmp r9, 2");                                           // kind 2 = indexed storage?
    emitter.instruction("jne __rt_array_key_exists_mixed_key_not_found");       // unknown kind → not found

    // -- indexed storage: dispatch on key tag --
    emitter.label("__rt_array_key_exists_mixed_key_indexed");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("cmp r11, -1");                                         // compare with -1 (int-key sentinel)
    emitter.instruction("jne __rt_array_key_exists_mixed_key_not_found");       // a string key never exists on packed/indexed storage

    // -- integer key on indexed storage: bounds-check only --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // r10 = key_lo (int index); caller-saved scratch only
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // r9 = array length (header offset 0)
    emitter.instruction("test r10, r10");                                       // negative index → not found
    emitter.instruction("js __rt_array_key_exists_mixed_key_not_found");        // negative indexed-array keys never exist
    emitter.instruction("cmp r10, r9");                                         // index >= length → not found
    emitter.instruction("jge __rt_array_key_exists_mixed_key_not_found");       // out-of-bounds indexed-array keys never exist
    emitter.instruction("mov rax, 1");                                          // in-bounds integer key → found
    emitter.instruction("jmp __rt_array_key_exists_mixed_key_done");            // skip the hash and not-found arms

    // -- hash storage: delegate to __rt_hash_get's found flag ---
    emitter.label("__rt_array_key_exists_mixed_key_hash");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = key_hi
    emitter.instruction("call __rt_hash_get");                                  // rax = found (a null-valued entry still reports found)
    emitter.instruction("jmp __rt_array_key_exists_mixed_key_done");            // hash_get's found flag is already the result

    // -- not found --
    emitter.label("__rt_array_key_exists_mixed_key_not_found");
    emitter.instruction("xor eax, eax");                                        // rax = not found

    emitter.label("__rt_array_key_exists_mixed_key_done");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return found flag in rax
}
