//! Purpose:
//! Emits `__rt_array_diff_str` and `__rt_array_intersect_str`: the value set operations over
//! STRING arrays, whose 16-byte `(ptr, len)` slots the 8-byte scalar helpers cannot read.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - One parameterised emitter serves both operations: the loop is identical and only the
//!   keep-on-match sense differs (diff keeps the UNMATCHED elements, intersect the matched).
//! - Elements are compared through `__rt_str_eq` and survivors re-persisted through
//!   `__rt_array_push_str`, so the result owns its bytes — the ownership pattern every string
//!   listing already uses.
//! - Like the scalar helpers, the result is REINDEXED 0..n; php preserves the source keys,
//!   which is the known divergence the whole family shares and is tracked separately.
//! - Every live value is reloaded from the frame around each call: `__rt_str_eq` and the
//!   append helper are free to clobber caller-saved registers, and an index kept in one across
//!   a call is the exact x86-only defect this repo has already shipped twice.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits one string set-operation helper; `keep_on_match` selects intersect over diff.
pub fn emit_array_set_op_str(emitter: &mut Emitter, label: &str, keep_on_match: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_op_str_linux_x86_64(emitter, label, keep_on_match);
        return;
    }

    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let matched = format!("{}_matched", label);
    let unmatched = format!("{}_unmatched", label);
    let keep = format!("{}_keep", label);
    let next = format!("{}_next", label);
    let done = format!("{}_done", label);

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // Frame: [0]=a [8]=b [16]=dest [24]=i [32]=j, linkage at [48].
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("str x0, [sp, #0]");                                    // the source array
    emitter.instruction("str x1, [sp, #8]");                                    // the comparison array
    emitter.instruction("ldr x0, [x0]");                                        // capacity = source length
    emitter.instruction("mov x1, #16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #16]");                                   // the destination array
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    emitter.label(&outer);
    emitter.instruction("ldr x9, [sp, #24]");                                   // i
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x11, [x10]");                                      // the source length
    emitter.instruction("cmp x9, x11");
    emitter.instruction(&format!("b.hs {}", done));                             // every element decided
    emitter.instruction("str xzr, [sp, #32]");                                  // j = 0

    emitter.label(&inner);
    emitter.instruction("ldr x9, [sp, #32]");                                   // j
    emitter.instruction("ldr x10, [sp, #8]");
    emitter.instruction("ldr x11, [x10]");                                      // the comparison length
    emitter.instruction("cmp x9, x11");
    emitter.instruction(&format!("b.hs {}", unmatched));                        // nothing in B matched A[i]
    // A[i] against B[j], both reloaded from their frames: the comparison clobbers freely.
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x12, [sp, #24]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x12, lsl #4");                           // A[i]'s slot
    emitter.instruction("ldr x1, [x10]");                                       // its pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // its length
    emitter.instruction("ldr x10, [sp, #8]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // B[j]'s slot
    emitter.instruction("ldr x3, [x10]");                                       // its pointer
    emitter.instruction("ldr x4, [x10, #8]");                                   // its length
    emitter.instruction("bl __rt_str_eq");                                      // x0 = 1 when the bytes agree
    emitter.instruction(&format!("cbnz x0, {}", matched));
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #32]");
    emitter.instruction(&format!("b {}", inner));

    emitter.label(&matched);
    if keep_on_match {
        emitter.instruction(&format!("b {}", keep));                            // intersect keeps a matched element
    } else {
        emitter.instruction(&format!("b {}", next));                            // diff drops it
    }
    emitter.label(&unmatched);
    if keep_on_match {
        emitter.instruction(&format!("b {}", next));                            // intersect drops an unmatched element
    } else {
        emitter.instruction(&format!("b {}", keep));                            // diff keeps it
    }

    emitter.label(&keep);
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x12, [sp, #24]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x12, lsl #4");                           // A[i]'s slot again
    emitter.instruction("ldr x1, [x10]");
    emitter.instruction("ldr x2, [x10, #8]");
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("bl __rt_array_push_str");                              // persist the survivor
    emitter.instruction("str x0, [sp, #16]");                                   // the append may have grown it

    emitter.label(&next);
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("add x9, x9, #1");                                      // i += 1
    emitter.instruction("str x9, [sp, #24]");
    emitter.instruction(&format!("b {}", outer));

    emitter.label(&done);
    emitter.instruction("ldr x0, [sp, #16]");                                   // the result
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// Emits `__rt_array_unique_str`: first occurrences of a string array, in order.
///
/// The inner loop compares A[i] against the RESULT built so far — equivalent to comparing
/// against A's own prefix, and it spares re-deciding elements that were already dropped.
/// Like the whole family, the result is REINDEXED where php preserves the source keys.
pub fn emit_array_unique_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unique_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unique_str ---");
    emitter.label_global("__rt_array_unique_str");

    // Frame: [0]=source [8]=dest [16]=i [24]=j, linkage at [32].
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("str x0, [sp, #0]");                                    // the source array
    emitter.instruction("ldr x0, [x0]");                                        // capacity = source length
    emitter.instruction("mov x1, #16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #8]");                                    // the destination array
    emitter.instruction("str xzr, [sp, #16]");                                  // i = 0

    emitter.label("__rt_aus_outer");
    emitter.instruction("ldr x9, [sp, #16]");                                   // i
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x11, [x10]");                                      // the source length
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.hs __rt_aus_done");                                  // every element decided
    emitter.instruction("str xzr, [sp, #24]");                                  // j = 0

    emitter.label("__rt_aus_inner");
    emitter.instruction("ldr x9, [sp, #24]");                                   // j
    emitter.instruction("ldr x10, [sp, #8]");
    emitter.instruction("ldr x11, [x10]");                                      // the kept count so far
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.hs __rt_aus_keep");                                  // nothing kept matches A[i]
    // A[i] against KEPT[j], both reloaded from their frames: the comparison clobbers freely.
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x12, [sp, #16]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x12, lsl #4");                           // A[i]'s slot
    emitter.instruction("ldr x1, [x10]");
    emitter.instruction("ldr x2, [x10, #8]");
    emitter.instruction("ldr x10, [sp, #8]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // KEPT[j]'s slot
    emitter.instruction("ldr x3, [x10]");
    emitter.instruction("ldr x4, [x10, #8]");
    emitter.instruction("bl __rt_str_eq");                                      // x0 = 1 when the bytes agree
    emitter.instruction("cbnz x0, __rt_aus_next");                              // a duplicate is dropped
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #24]");
    emitter.instruction("b __rt_aus_inner");

    emitter.label("__rt_aus_keep");
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x12, [sp, #16]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x12, lsl #4");                           // A[i]'s slot again
    emitter.instruction("ldr x1, [x10]");
    emitter.instruction("ldr x2, [x10, #8]");
    emitter.instruction("ldr x0, [sp, #8]");
    emitter.instruction("bl __rt_array_push_str");                              // persist the first occurrence
    emitter.instruction("str x0, [sp, #8]");                                    // the append may have grown it

    emitter.label("__rt_aus_next");
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("add x9, x9, #1");                                      // i += 1
    emitter.instruction("str x9, [sp, #16]");
    emitter.instruction("b __rt_aus_outer");

    emitter.label("__rt_aus_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the deduplicated array
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// Emits the x86_64 form of [`emit_array_unique_str`].
fn emit_array_unique_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unique_str ---");
    emitter.label_global("__rt_array_unique_str");

    // Frame: [rbp-8]=source [rbp-16]=dest [rbp-24]=i [rbp-32]=j.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the source array
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // capacity = source length
    emitter.instruction("mov rsi, 16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the destination array
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // i = 0

    emitter.label("__rt_aus_outer_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // i
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // against the source length
    emitter.instruction("jae __rt_aus_done_x");                                 // every element decided
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // j = 0

    emitter.label("__rt_aus_inner_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // j
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // against the kept count so far
    emitter.instruction("jae __rt_aus_keep_x");                                 // nothing kept matches A[i]
    // A[i] against KEPT[j], both reloaded from their frames: the comparison clobbers freely.
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");
    emitter.instruction("shl r11, 4");
    emitter.instruction("lea r10, [r10 + r11 + 24]");                           // A[i]'s slot
    emitter.instruction("mov rdi, QWORD PTR [r10]");
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction("shl r9, 4");
    emitter.instruction("lea r10, [r10 + r9 + 24]");                            // KEPT[j]'s slot
    emitter.instruction("mov rdx, QWORD PTR [r10]");
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");
    emitter.instruction("call __rt_str_eq");                                    // rax = 1 when the bytes agree
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_aus_next_x");                                 // a duplicate is dropped
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");
    emitter.instruction("add r9, 1");                                           // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");
    emitter.instruction("jmp __rt_aus_inner_x");

    emitter.label("__rt_aus_keep_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");
    emitter.instruction("shl r11, 4");
    emitter.instruction("lea r10, [r10 + r11 + 24]");                           // A[i]'s slot again
    emitter.instruction("mov rsi, QWORD PTR [r10]");
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_array_push_str");                            // persist the first occurrence
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the append may have grown it

    emitter.label("__rt_aus_next_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");
    emitter.instruction("jmp __rt_aus_outer_x");

    emitter.label("__rt_aus_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the deduplicated array
    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits `__rt_array_merge_str`: append two string arrays into a new one.
///
/// Same slot story as the set operations: 16-byte descriptors, elements re-persisted so the
/// result owns its bytes. php's `array_merge` reindexes list keys, which is exactly what two
/// append loops produce — no key divergence here, unlike the set operations.
pub fn emit_array_merge_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    // Frame: [0]=source being copied [8]=second source [16]=dest [24]=i, linkage at [32].
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("str x0, [sp, #0]");                                    // the first array
    emitter.instruction("str x1, [sp, #8]");                                    // the second array
    emitter.instruction("ldr x9, [x0]");
    emitter.instruction("ldr x10, [x1]");
    emitter.instruction("add x0, x9, x10");                                     // capacity = both lengths
    emitter.instruction("mov x1, #16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #16]");                                   // the destination array
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    // -- copy the current source, then swap in the second and run once more --
    emitter.label("__rt_ams_copy");
    emitter.instruction("ldr x9, [sp, #24]");                                   // i
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("ldr x11, [x10]");                                      // the current source length
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.hs __rt_ams_advance");                               // this source is exhausted
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // this slot's address
    emitter.instruction("ldr x1, [x10]");                                       // the string pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // and its length
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("bl __rt_array_push_str");                              // persist into the destination
    emitter.instruction("str x0, [sp, #16]");
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #24]");
    emitter.instruction("b __rt_ams_copy");

    emitter.label("__rt_ams_advance");
    emitter.instruction("ldr x9, [sp, #8]");                                    // the second source, if any remains
    emitter.instruction("cbz x9, __rt_ams_done");
    emitter.instruction("str x9, [sp, #0]");                                    // it becomes the current source
    emitter.instruction("str xzr, [sp, #8]");                                   // and there is no third
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0 for the second pass
    emitter.instruction("b __rt_ams_copy");

    emitter.label("__rt_ams_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the merged array
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// Emits the x86_64 form of [`emit_array_merge_str`].
fn emit_array_merge_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_str ---");
    emitter.label_global("__rt_array_merge_str");

    // Frame: [rbp-8]=source being copied [rbp-16]=second source [rbp-24]=dest [rbp-32]=i.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the first array
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the second array
    emitter.instruction("mov r9, QWORD PTR [rdi]");
    emitter.instruction("add r9, QWORD PTR [rsi]");                             // capacity = both lengths
    emitter.instruction("mov rdi, r9");
    emitter.instruction("mov rsi, 16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the destination array
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // i = 0

    emitter.label("__rt_ams_copy_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // i
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // against the current source length
    emitter.instruction("jae __rt_ams_advance_x");                              // this source is exhausted
    emitter.instruction("shl r9, 4");
    emitter.instruction("lea r10, [r10 + r9 + 24]");                            // this slot's address
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // the string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // and its length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_array_push_str");                            // persist into the destination
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");
    emitter.instruction("add r9, 1");
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");
    emitter.instruction("jmp __rt_ams_copy_x");

    emitter.label("__rt_ams_advance_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // the second source, if any remains
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_ams_done_x");
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // it becomes the current source
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // and there is no third
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // i = 0 for the second pass
    emitter.instruction("jmp __rt_ams_copy_x");

    emitter.label("__rt_ams_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the merged array
    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits the x86_64 form of [`emit_array_set_op_str`].
fn emit_array_set_op_str_linux_x86_64(emitter: &mut Emitter, label: &str, keep_on_match: bool) {
    let outer = format!("{}_outer_x", label);
    let inner = format!("{}_inner_x", label);
    let matched = format!("{}_matched_x", label);
    let unmatched = format!("{}_unmatched_x", label);
    let keep = format!("{}_keep_x", label);
    let next = format!("{}_next_x", label);
    let done = format!("{}_done_x", label);

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // Frame: [rbp-8]=a [rbp-16]=b [rbp-24]=dest [rbp-32]=i [rbp-40]=j.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the source array
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the comparison array
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // capacity = source length
    emitter.instruction("mov rsi, 16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the destination array
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // i = 0

    emitter.label(&outer);
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // i
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // against the source length
    emitter.instruction(&format!("jae {}", done));                              // every element decided
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // j = 0

    emitter.label(&inner);
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // j
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // against the comparison length
    emitter.instruction(&format!("jae {}", unmatched));                         // nothing in B matched A[i]
    // A[i] against B[j], both reloaded from their frames: the comparison clobbers freely.
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");
    emitter.instruction("shl r11, 4");
    emitter.instruction("lea r10, [r10 + r11 + 24]");                           // A[i]'s slot
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // its pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // its length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction("shl r9, 4");
    emitter.instruction("lea r10, [r10 + r9 + 24]");                            // B[j]'s slot
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // its pointer
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // its length
    emitter.instruction("call __rt_str_eq");                                    // rax = 1 when the bytes agree
    emitter.instruction("test rax, rax");
    emitter.instruction(&format!("jnz {}", matched));
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");
    emitter.instruction("add r9, 1");                                           // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");
    emitter.instruction(&format!("jmp {}", inner));

    emitter.label(&matched);
    if keep_on_match {
        emitter.instruction(&format!("jmp {}", keep));                          // intersect keeps a matched element
    } else {
        emitter.instruction(&format!("jmp {}", next));                          // diff drops it
    }
    emitter.label(&unmatched);
    if keep_on_match {
        emitter.instruction(&format!("jmp {}", next));                          // intersect drops an unmatched element
    } else {
        emitter.instruction(&format!("jmp {}", keep));                          // diff keeps it
    }

    emitter.label(&keep);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");
    emitter.instruction("shl r11, 4");
    emitter.instruction("lea r10, [r10 + r11 + 24]");                           // A[i]'s slot again
    emitter.instruction("mov rsi, QWORD PTR [r10]");
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_array_push_str");                            // persist the survivor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the append may have grown it

    emitter.label(&next);
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");
    emitter.instruction(&format!("jmp {}", outer));

    emitter.label(&done);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the result
    emitter.instruction("add rsp, 48");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
