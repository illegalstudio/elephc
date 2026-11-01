//! Purpose:
//! Emits the Linux x86_64 runtime loop for `array_filter()` over a boxed gradual array.
//! Preserves PHP keys, insertion order, and retained child ownership in a unified result hash.
//!
//! Called from:
//! - `super::emit_array_filter_mixed()` for the Linux x86_64 target.
//!
//! Key details:
//! - Callback-facing values and keys are temporary owned `Mixed` cells released after each call.
//! - Kept hash entries copy the original payload so PHP reference cells remain shared.

use crate::codegen_support::emit::Emitter;

use super::ARRAY_FILTER_MODE_MSG_LEN;
use super::super::value_error;

/// Emits the Linux x86_64 `__rt_array_filter_mixed` helper.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_mixed ---");
    emitter.label_global("__rt_array_filter_mixed");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                       // establish stable spill-slot addressing
    emitter.instruction("sub rsp, 160");                                       // reserve aligned filter state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                       // save callback address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                      // save boxed source array
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                      // save optional callback environment
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                      // save callback mode

    emitter.instruction("cmp rcx, 0");                                         // accept value-only mode
    emitter.instruction("je __rt_array_filter_mixed_mode_valid_x86");
    emitter.instruction("cmp rcx, 1");                                         // accept value-and-key mode
    emitter.instruction("je __rt_array_filter_mixed_mode_valid_x86");
    emitter.instruction("cmp rcx, 2");                                         // accept key-only mode
    emitter.instruction("je __rt_array_filter_mixed_mode_valid_x86");
    emitter.instruction("jmp __rt_array_filter_mixed_invalid_mode_x86");       // reject every other mode with ValueError
    emitter.label("__rt_array_filter_mixed_mode_valid_x86");

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                      // reload boxed source for runtime dispatch
    emitter.instruction("call __rt_mixed_unbox");                              // rax=tag, rdi=payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                      // save source runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                      // save borrowed source payload pointer
    emitter.instruction("cmp rax, 4");                                         // tag 4 = indexed array
    emitter.instruction("je __rt_array_filter_mixed_indexed_setup_x86");
    emitter.instruction("jmp __rt_array_filter_mixed_hash_setup_x86");         // codegen guard permits only tag 5 otherwise

    emitter.label("__rt_array_filter_mixed_indexed_setup_x86");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                           // snapshot indexed source length
    emitter.instruction("mov QWORD PTR [rbp - 136], r10");                    // preserve source length across callbacks
    emitter.instruction("mov r11, QWORD PTR [rdi + 16]");                      // load indexed element width
    emitter.instruction("mov QWORD PTR [rbp - 144], r11");                    // preserve element width
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                        // load packed source metadata
    emitter.instruction("shr r9, 8");                                          // move runtime element tag into low bits
    emitter.instruction("and r9, 0x7f");                                       // isolate runtime element tag
    emitter.instruction("mov QWORD PTR [rbp - 152], r9");                     // preserve source value tag
    emitter.instruction("cmp r10, 4");                                         // hash insertion needs a non-zero minimum capacity
    emitter.instruction("jge __rt_array_filter_mixed_indexed_capacity_ready_x86");
    emitter.instruction("mov r10, 4");
    emitter.label("__rt_array_filter_mixed_indexed_capacity_ready_x86");
    emitter.instruction("mov rdi, r10");                                       // destination capacity follows source length
    emitter.instruction("mov esi, 7");                                         // filtered entries may be heterogeneous
    emitter.instruction("call __rt_hash_new");                                 // hash storage preserves original numeric keys
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                     // save destination hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                       // source index starts at zero

    emitter.label("__rt_array_filter_mixed_indexed_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                     // reload source index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 136]");                    // compare against snapshotted length
    emitter.instruction("jge __rt_array_filter_mixed_indexed_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                     // raw indexed source pointer
    emitter.instruction("mov rsi, r10");                                       // integer key low word
    emitter.instruction("mov rdx, -1");                                        // integer-key sentinel
    emitter.instruction("xor ecx, ecx");                                       // known-present iterator reads do not warn
    emitter.instruction("call __rt_array_get_mixed_key");                      // owned callback-facing Mixed value
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                    // preserve callback value
    emitter.instruction("mov QWORD PTR [rbp - 120], 0");                      // default to no key box
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                       // value-only mode?
    emitter.instruction("je __rt_array_filter_mixed_indexed_call_x86");
    emitter.instruction("xor eax, eax");                                       // runtime tag 0 = integer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                     // key payload is source index
    emitter.instruction("xor esi, esi");                                       // integer key has no high word
    emitter.instruction("call __rt_mixed_from_value");                         // allocate callback-facing key cell
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                    // preserve owned key cell

    emitter.label("__rt_array_filter_mixed_indexed_call_x86");
    emit_callback_call(
        emitter,
        "__rt_array_filter_mixed_indexed_callback_x86",
        "__rt_array_filter_mixed_indexed_after_call_x86",
    );
    emitter.label("__rt_array_filter_mixed_indexed_after_call_x86");
    emit_release_callback_values(emitter);
    emitter.instruction("cmp QWORD PTR [rbp - 128], 0");                      // did callback reject this element?
    emitter.instruction("je __rt_array_filter_mixed_indexed_next_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 144], 16");                     // string slot layout?
    emitter.instruction("je __rt_array_filter_mixed_indexed_value_string_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                     // raw source array
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                     // source index
    emitter.instruction("mov rcx, QWORD PTR [r10 + r11 * 8 + 24]");            // original scalar/refcounted payload
    emitter.instruction("xor r8d, r8d");                                       // non-string payloads have no high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 152]");                     // preserve source value tag
    emitter.instruction("jmp __rt_array_filter_mixed_indexed_value_ready_x86");
    emitter.label("__rt_array_filter_mixed_indexed_value_string_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                     // raw source array
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                     // source index
    emitter.instruction("shl r11, 4");                                         // string slots are 16 bytes wide
    emitter.instruction("add r10, r11");
    emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");                     // source string pointer
    emitter.instruction("mov r8, QWORD PTR [r10 + 32]");                      // source string length
    emitter.instruction("mov r9d, 1");                                         // runtime tag 1 = string
    emitter.label("__rt_array_filter_mixed_indexed_value_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], rcx");                     // preserve original value low word
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                      // preserve original value high word
    emitter.instruction("mov QWORD PTR [rbp - 104], r9");                     // preserve original value tag
    emitter.instruction("cmp r9, 1");                                          // string payload requires a retained owner
    emitter.instruction("je __rt_array_filter_mixed_indexed_retain_x86");
    emitter.instruction("cmp r9, 4");                                          // indexed child pointer?
    emitter.instruction("jl __rt_array_filter_mixed_indexed_insert_x86");
    emitter.instruction("cmp r9, 7");                                          // array/hash/object/Mixed pointer range?
    emitter.instruction("jle __rt_array_filter_mixed_indexed_retain_x86");
    emitter.instruction("cmp r9, 10");                                         // callable descriptor pointer?
    emitter.instruction("je __rt_array_filter_mixed_indexed_retain_x86");
    emitter.instruction("cmp r9, 11");                                         // PHP reference cell pointer?
    emitter.instruction("jne __rt_array_filter_mixed_indexed_insert_x86");
    emitter.label("__rt_array_filter_mixed_indexed_retain_x86");
    emitter.instruction("mov rax, rcx");                                       // borrowed source child pointer
    emitter.instruction("call __rt_incref");                                   // result hash takes an owning share
    emitter.label("__rt_array_filter_mixed_indexed_insert_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                     // destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                     // preserve original integer key
    emitter.instruction("mov rdx, -1");                                        // integer-key sentinel
    emitter.instruction("mov rcx, QWORD PTR [rbp - 88]");                     // original value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                      // original value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                     // original value tag
    emitter.instruction("call __rt_hash_set");                                 // transfer retained payload into filtered hash
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                     // save possibly grown destination pointer
    emitter.label("__rt_array_filter_mixed_indexed_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 64], 1");                       // advance source index
    emitter.instruction("jmp __rt_array_filter_mixed_indexed_loop_x86");

    emitter.label("__rt_array_filter_mixed_indexed_done_x86");
    emit_box_result_hash(emitter);
    emitter.instruction("jmp __rt_array_filter_mixed_return_x86");

    emitter.label("__rt_array_filter_mixed_hash_setup_x86");
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                       // source hash capacity
    emitter.instruction("mov esi, 7");                                         // destination may contain heterogeneous values
    emitter.instruction("call __rt_hash_new");                                 // allocate destination hash
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                     // save destination hash
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                       // insertion-order cursor starts at zero

    emitter.label("__rt_array_filter_mixed_hash_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                     // source hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                     // insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                           // fetch key and original payload tuple
    emitter.instruction("cmp rax, -1");
    emitter.instruction("je __rt_array_filter_mixed_hash_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                     // save next cursor
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                     // key low word
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                     // key high word/sentinel
    emitter.instruction("mov QWORD PTR [rbp - 88], rcx");                     // original value low word
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                      // original value high word
    emitter.instruction("mov QWORD PTR [rbp - 104], r9");                     // original value tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                     // raw source hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                     // current key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                     // current key high word
    emitter.instruction("xor ecx, ecx");                                       // known-present iterator reads do not warn
    emitter.instruction("call __rt_array_get_mixed_key");                      // owned, dereferenced callback value
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                    // preserve callback value
    emitter.instruction("mov QWORD PTR [rbp - 120], 0");                      // default to no callback key
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                       // value-only callback?
    emitter.instruction("je __rt_array_filter_mixed_hash_call_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                     // key low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                     // key length/sentinel
    emitter.instruction("cmp rsi, -1");                                        // classify integer versus string key
    emitter.instruction("jne __rt_array_filter_mixed_hash_key_string_x86");
    emitter.instruction("xor eax, eax");                                       // runtime tag 0 = integer
    emitter.instruction("xor esi, esi");                                       // integer keys have no high word
    emitter.instruction("jmp __rt_array_filter_mixed_hash_key_box_x86");
    emitter.label("__rt_array_filter_mixed_hash_key_string_x86");
    emitter.instruction("mov eax, 1");                                         // runtime tag 1 = string
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                     // restore string key length
    emitter.label("__rt_array_filter_mixed_hash_key_box_x86");
    emitter.instruction("call __rt_mixed_from_value");                         // allocate callback-facing key cell
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                    // preserve owned key cell

    emitter.label("__rt_array_filter_mixed_hash_call_x86");
    emit_callback_call(
        emitter,
        "__rt_array_filter_mixed_hash_callback_x86",
        "__rt_array_filter_mixed_hash_after_call_x86",
    );
    emitter.label("__rt_array_filter_mixed_hash_after_call_x86");
    emit_release_callback_values(emitter);
    emitter.instruction("cmp QWORD PTR [rbp - 128], 0");                      // did callback reject this entry?
    emitter.instruction("je __rt_array_filter_mixed_hash_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                    // original payload runtime tag
    emitter.instruction("cmp r10, 1");                                         // string payload requires a retained owner
    emitter.instruction("je __rt_array_filter_mixed_hash_retain_x86");
    emitter.instruction("cmp r10, 4");                                         // indexed child pointer?
    emitter.instruction("jl __rt_array_filter_mixed_hash_insert_x86");
    emitter.instruction("cmp r10, 7");                                         // array/hash/object/Mixed pointer range?
    emitter.instruction("jle __rt_array_filter_mixed_hash_retain_x86");
    emitter.instruction("cmp r10, 10");                                        // callable descriptor pointer?
    emitter.instruction("je __rt_array_filter_mixed_hash_retain_x86");
    emitter.instruction("cmp r10, 11");                                        // shared PHP reference cell?
    emitter.instruction("jne __rt_array_filter_mixed_hash_insert_x86");
    emitter.label("__rt_array_filter_mixed_hash_retain_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                     // borrowed source child pointer
    emitter.instruction("call __rt_incref");                                   // result hash takes an owning share
    emitter.label("__rt_array_filter_mixed_hash_insert_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                     // destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                     // original key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                     // original key high word/sentinel
    emitter.instruction("mov rcx, QWORD PTR [rbp - 88]");                     // original value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                      // original value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                     // original value tag, including references
    emitter.instruction("call __rt_hash_set");                                 // persist key and transfer retained value ownership
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                     // save possibly grown destination hash
    emitter.instruction("jmp __rt_array_filter_mixed_hash_loop_x86");

    emitter.label("__rt_array_filter_mixed_hash_done_x86");
    emit_box_result_hash(emitter);

    emitter.label("__rt_array_filter_mixed_return_x86");
    emitter.instruction("add rsp, 160");                                       // release filter spill slots
    emitter.instruction("pop rbp");                                            // restore caller frame pointer
    emitter.instruction("ret");                                                // return boxed filtered array

    emitter.label("__rt_array_filter_mixed_invalid_mode_x86");
    // The shared ValueError thrower opens its own frame assuming it was spliced at a function
    // ENTRY, where System V leaves rsp 8 bytes past a 16-byte boundary. This helper is 168
    // bytes into its frame here, so the throw path first returns the stack to the entry state
    // — it never comes back, and an unaligned `__rt_heap_alloc` below faults once the
    // allocator reaches real C.
    emitter.instruction("mov rsp, rbp");                                       // drop this helper's spill slots before the shared throw sequence
    emitter.instruction("pop rbp");                                            // restore the caller frame pointer, leaving rsp as it was at entry
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_array_filter_mode_msg",
        ARRAY_FILTER_MODE_MSG_LEN,
    );

    emitter.blank();
    emitter.comment("--- runtime: array_filter_mixed_raw ---");
    emitter.label_global("__rt_array_filter_mixed_raw");
    emitter.instruction("push rbp");                                           // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                      // establish stable wrapper frame
    emitter.instruction("sub rsp, 48");                                       // reserve arguments, temporary box, and result
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                      // callback address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                     // borrowed raw array/hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                     // optional callback environment
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                     // callback mode
    emitter.instruction("mov rax, rsi");                                      // pass raw container to kind-aware boxer
    emitter.instruction("call __rt_mixed_from_array_kind");                   // create owned temporary source box
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                     // preserve temporary source box
    emitter.instruction("mov rsi, rax");                                      // boxed source is main helper's second argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                      // reload callback address
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                     // reload optional environment
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                     // reload callback mode
    emitter.instruction("call __rt_array_filter_mixed");                      // filter through boxed dynamic path
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                     // preserve boxed filtered result
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                     // temporary source box
    emitter.instruction("call __rt_decref_mixed");                            // release temporary source box and retained child
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                     // restore boxed filtered result
    emitter.instruction("add rsp, 48");                                       // release wrapper spill slots
    emitter.instruction("pop rbp");                                           // restore caller frame pointer
    emitter.instruction("ret");                                               // return boxed filtered result
}

/// Emits mode-sensitive SysV callback argument setup and the indirect call.
fn emit_callback_call(emitter: &mut Emitter, prefix: &str, return_label: &str) {
    let key_label = format!("{}_key", prefix);
    let both_label = format!("{}_both", prefix);
    let ready_label = format!("{}_ready", prefix);
    emitter.instruction("cmp QWORD PTR [rbp - 32], 2");                       // key-only callback?
    emitter.instruction(&format!("je {}", key_label));
    emitter.instruction("cmp QWORD PTR [rbp - 32], 1");                       // value-and-key callback?
    emitter.instruction(&format!("je {}", both_label));
    emitter.instruction("mov rdi, QWORD PTR [rbp - 112]");                    // value-only callback argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                     // optional environment
    emitter.instruction("test r10, r10");
    emitter.instruction(&format!("jz {}", ready_label));
    emitter.instruction("mov rsi, r10");                                       // environment follows value
    emitter.instruction(&format!("jmp {}", ready_label));
    emitter.label(&both_label);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 112]");                    // value argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 120]");                    // key argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                     // optional environment
    emitter.instruction("test r10, r10");
    emitter.instruction(&format!("jz {}", ready_label));
    emitter.instruction("mov rdx, r10");                                       // environment follows value and key
    emitter.instruction(&format!("jmp {}", ready_label));
    emitter.label(&key_label);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 120]");                    // key-only callback argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                     // optional environment
    emitter.instruction("test r10, r10");
    emitter.instruction(&format!("jz {}", ready_label));
    emitter.instruction("mov rsi, r10");                                       // environment follows key
    emitter.label(&ready_label);
    emitter.instruction("call QWORD PTR [rbp - 8]");                          // invoke predicate; truthiness returns in rax
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                    // preserve callback result across releases
    emitter.instruction(&format!("jmp {}", return_label));
}

/// Releases the owned callback-facing value and optional key cells.
fn emit_release_callback_values(emitter: &mut Emitter) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                    // owned callback value cell
    emitter.instruction("call __rt_decref_mixed");                             // release value after predicate returns
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                    // optional owned callback key cell
    emitter.instruction("call __rt_decref_mixed");                             // null-safe release of key cell
}

/// Boxes the owned result hash as `Mixed` and transfers its raw ownership into the box.
fn emit_box_result_hash(emitter: &mut Emitter) {
    emitter.instruction("mov eax, 5");                                         // runtime tag 5 = associative array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                     // raw filtered hash payload
    emitter.instruction("xor esi, esi");
    emitter.instruction("call __rt_mixed_from_value");                         // result box retains the raw hash
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                    // preserve result box across ownership transfer
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                     // helper's original raw-hash owner
    emitter.instruction("call __rt_decref_any");                               // release original raw-hash ownership
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                    // restore boxed result
}
