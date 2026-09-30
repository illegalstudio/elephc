//! Purpose:
//! Emits `__rt_brigade_remove`, which takes one bucket OUT of a brigade before it is put back.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::stream_bucket_arch`, on both the append and the
//!   prepend paths, between the bucket's `incref` and the array push.
//!
//! Key details:
//! - A brigade holds each bucket AT MOST ONCE. php-src's buckets are a linked list and appending
//!   one that is already linked MOVES it; elephc's brigade is an array, so the move has to be
//!   spelled out as "take it out, then put it back at the requested end".
//! - MEASURED on `php -n` 8.5.6, a write filter over `"abc"` (`scratchpad/qp/a/bucket2.php`,
//!   `bucket3.php`):
//!
//!   ```text
//!       append the same bucket three times          php 'ABC'   elephc 'ABCABCABC'
//!       append, set data = "ZZZ", append again      php 'ZZZ'   elephc 'ZZZZZZ'
//!       append then PREPEND the same bucket         php 'abc'   elephc 'abcabc'
//!   ```
//!
//!   The second line is what pins the rule down: php answers `'ZZZ'` and not `'abcZZZ'`, so the
//!   brigade never held two entries — one entry, read at flush time, showing whatever the object
//!   says by then. A filter that appends twice, which php-src's own `filters/bug35916.phpt` does
//!   deliberately, emitted its payload twice here.
//! - Buckets are compared by their OBJECT pointer, not by the Mixed cell that carries them: the
//!   cell is whatever the caller's local holds and could be re-boxed between the two calls, while
//!   the object behind it is the identity php is tracking.
//! - The removed entry is decref'd. The caller increfs BEFORE calling this, so the count cannot
//!   reach zero here and free the very bucket about to be pushed back.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Byte offset of a packed array's element payload, past its header.
const ARRAY_PAYLOAD_OFFSET: i64 = 24;

/// Byte offset of the payload word inside a Mixed cell; `[cell + 0]` is its tag.
const MIXED_PAYLOAD_OFFSET: i64 = 8;

/// Emits `__rt_brigade_remove(buckets, cell) -> buckets`.
///
/// # Input / Output
/// - AArch64: `x0` the `_buckets` packed array, `x1` the bucket's Mixed cell. Answers `x0`.
/// - x86_64: `rdi` the array, `rsi` the cell. Answers `rax`.
///
/// The array is never reallocated, so the pointer in equals the pointer out; only its length word
/// and element slots change. A brigade that does not hold this bucket is left exactly as it was.
pub fn emit_brigade_remove(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: take one bucket out of a brigade ---");
    emitter.label_global("__rt_brigade_remove");
    emitter.instruction("sub sp, sp, #32");                                     // frame for the array across the decref
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // the array is the answer, whatever happens
    emitter.instruction("cbz x0, __rt_brem_done");                              // no array: nothing to take out
    emitter.instruction("cbz x1, __rt_brem_done");                              // no bucket: nothing to look for
    emitter.instruction(&format!("ldr x11, [x1, #{MIXED_PAYLOAD_OFFSET}]"));    // the object behind the incoming cell
    emitter.instruction("cbz x11, __rt_brem_done");                             // an empty cell matches nothing
    emitter.instruction("ldr x9, [x0]");                                        // how many buckets the brigade holds
    emitter.instruction(&format!("add x10, x0, #{ARRAY_PAYLOAD_OFFSET}"));      // the element payload base
    emitter.instruction("mov x12, #0");                                         // scan index

    emitter.label("__rt_brem_scan");
    emitter.instruction("cmp x12, x9");
    emitter.instruction("b.hs __rt_brem_done");                                 // scanned every slot: this bucket is new here
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                         // the cell parked in this slot
    emitter.instruction("cbz x13, __rt_brem_next");
    emitter.instruction(&format!("ldr x14, [x13, #{MIXED_PAYLOAD_OFFSET}]"));   // the object behind it
    emitter.instruction("cmp x14, x11");                                        // the same bucket?
    emitter.instruction("b.eq __rt_brem_found");
    emitter.label("__rt_brem_next");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_brem_scan");

    emitter.label("__rt_brem_found");
    emitter.instruction("str x13, [sp, #8]");                                   // the cell whose reference the array drops
    emitter.instruction("sub x9, x9, #1");                                      // one fewer bucket from here on
    emitter.label("__rt_brem_shift");
    emitter.instruction("cmp x12, x9");
    emitter.instruction("b.hs __rt_brem_shrink");                               // the hole reached the tail
    emitter.instruction("add x15, x12, #1");
    emitter.instruction("ldr x13, [x10, x15, lsl #3]");                         // pull the next bucket back one slot
    emitter.instruction("str x13, [x10, x12, lsl #3]");
    emitter.instruction("mov x12, x15");
    emitter.instruction("b __rt_brem_shift");
    emitter.label("__rt_brem_shrink");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the array
    emitter.instruction("str x9, [x0]");                                        // publish the shortened length
    emitter.instruction("ldr x0, [sp, #8]");                                    // release what the array was holding
    emitter.instruction("bl __rt_decref_any");                                  // safe: the caller increfs before this

    emitter.label("__rt_brem_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // answer the array, moved or not
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");
}

fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: take one bucket out of a brigade ---");
    emitter.label_global("__rt_brigade_remove");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame for the array across the decref
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the array is the answer, whatever happens
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_brem_done_x86");                               // no array: nothing to take out
    emitter.instruction("test rsi, rsi");
    emitter.instruction("jz __rt_brem_done_x86");                               // no bucket: nothing to look for
    emitter.instruction(&format!("mov r11, QWORD PTR [rsi + {MIXED_PAYLOAD_OFFSET}]")); // the object behind the incoming cell
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_brem_done_x86");                               // an empty cell matches nothing
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // how many buckets the brigade holds
    emitter.instruction(&format!("lea r10, [rdi + {ARRAY_PAYLOAD_OFFSET}]"));   // the element payload base
    emitter.instruction("xor rcx, rcx");                                        // scan index

    emitter.label("__rt_brem_scan_x86");
    emitter.instruction("cmp rcx, r9");
    emitter.instruction("jae __rt_brem_done_x86");                              // scanned every slot: this bucket is new here
    emitter.instruction("mov r8, QWORD PTR [r10 + rcx * 8]");                   // the cell parked in this slot
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_brem_next_x86");
    emitter.instruction(&format!("cmp r11, QWORD PTR [r8 + {MIXED_PAYLOAD_OFFSET}]")); // the same bucket?
    emitter.instruction("je __rt_brem_found_x86");
    emitter.label("__rt_brem_next_x86");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_brem_scan_x86");

    emitter.label("__rt_brem_found_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // the cell whose reference the array drops
    emitter.instruction("dec r9");                                              // one fewer bucket from here on
    emitter.label("__rt_brem_shift_x86");
    emitter.instruction("cmp rcx, r9");
    emitter.instruction("jae __rt_brem_shrink_x86");                            // the hole reached the tail
    emitter.instruction("mov r8, QWORD PTR [r10 + rcx * 8 + 8]");               // pull the next bucket back one slot
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], r8");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_brem_shift_x86");
    emitter.label("__rt_brem_shrink_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the array
    emitter.instruction("mov QWORD PTR [rax], r9");                             // publish the shortened length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // release what the array was holding
    emitter.instruction("call __rt_decref_any");                                // safe: the caller increfs before this

    emitter.label("__rt_brem_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // answer the array, moved or not
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
