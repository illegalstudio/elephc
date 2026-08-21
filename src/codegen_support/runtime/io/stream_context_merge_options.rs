//! Purpose:
//! Emits PHP stream-context option merging for wrapper-keyed option hashes.
//! Mirrors php-src's two-level `parse_context_options()` walk without recursively
//! merging array-valued option payloads.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - Stream-context create and mutation lowering through
//!   `__rt_stream_context_merge_options`.
//!
//! Key details:
//! - The top-level wrapper map is cloned before mutation.
//! - Each wrapper's option map uses shallow right-wins `array_replace` semantics.
//! - Inputs stay untouched; the returned hash is a new owned COW root.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific two-level stream-context options merge helper.
///
/// Input: AArch64 `x0=existing`, `x1=incoming`; x86_64
/// `rdi=existing`, `rsi=incoming`. Either top-level input may be null.
/// Output: one newly owned merged hash in the target integer result register.
pub(crate) fn emit_stream_context_merge_options(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_stream_context_merge_options_aarch64(emitter),
        Arch::X86_64 => emit_stream_context_merge_options_x86_64(emitter),
    }
}

/// Emits the AArch64 wrapper-to-options merge implementation.
fn emit_stream_context_merge_options_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: merge stream-context wrapper options ---");
    emitter.label_global("__rt_stream_context_merge_options");
    emitter.instruction("sub sp, sp, #128");                                    // reserve merge iteration state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #112");                                   // establish a stable helper frame
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the incoming wrapper map
    emitter.instruction("cbz x0, __rt_sc_merge_clone_incoming");                // an empty existing context starts as an incoming clone
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone the existing wrapper map for COW mutation
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the owned result wrapper map
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize incoming-wrapper iteration
    emitter.label("__rt_sc_merge_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the incoming wrapper map
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload its insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch wrapper key and option-map payload
    emitter.instruction("cmn x0, #1");                                          // did the iterator reach its end sentinel?
    emitter.instruction("b.eq __rt_sc_merge_done");                             // finish after every incoming wrapper
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the next incoming-wrapper cursor
    emitter.instruction("str x1, [sp, #24]");                                   // preserve the wrapper key pointer
    emitter.instruction("str x2, [sp, #32]");                                   // preserve the wrapper key length
    emitter.instruction("cmp x5, #5");                                          // is the incoming wrapper value an associative option map?
    emitter.instruction("b.eq __rt_sc_merge_incoming_direct");                  // direct hash payloads already expose their pointer
    emitter.instruction("cmp x5, #7");                                          // is the wrapper value a boxed Mixed payload?
    emitter.instruction("b.ne __rt_sc_merge_loop");                             // ignore malformed non-array wrapper values
    emitter.instruction("mov x0, x3");                                          // pass the boxed wrapper value to Mixed unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose its concrete tag and payload words
    emitter.instruction("cmp x0, #5");                                          // did the boxed value contain an associative option map?
    emitter.instruction("b.ne __rt_sc_merge_loop");                             // ignore malformed boxed wrapper values
    emitter.instruction("str x1, [sp, #40]");                                   // preserve the unboxed incoming option-map pointer
    emitter.instruction("b __rt_sc_merge_find_existing");                       // continue with existing-wrapper lookup
    emitter.label("__rt_sc_merge_incoming_direct");
    emitter.instruction("str x3, [sp, #40]");                                   // preserve the direct incoming option-map pointer
    emitter.label("__rt_sc_merge_find_existing");
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the result wrapper map for lookup
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the wrapper key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the wrapper key length
    emitter.instruction("bl __rt_hash_get");                                    // find the current option map for this wrapper
    emitter.instruction("cbz x0, __rt_sc_merge_clone_sub");                     // a new wrapper starts as a clone of its incoming map
    emitter.instruction("cmp x3, #5");                                          // is the current wrapper value a direct associative map?
    emitter.instruction("b.eq __rt_sc_merge_existing_direct");                  // use its direct payload pointer
    emitter.instruction("cmp x3, #7");                                          // is the current wrapper value boxed as Mixed?
    emitter.instruction("b.ne __rt_sc_merge_clone_sub");                        // malformed current values are replaced by the incoming map
    emitter.instruction("mov x0, x1");                                          // pass the boxed current option map to Mixed unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose its concrete tag and payload words
    emitter.instruction("cmp x0, #5");                                          // did the box contain an associative option map?
    emitter.instruction("b.ne __rt_sc_merge_clone_sub");                        // malformed current values are replaced
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the unboxed current option-map pointer
    emitter.instruction("b __rt_sc_merge_replace_sub");                         // shallow-merge the two option maps
    emitter.label("__rt_sc_merge_existing_direct");
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the direct current option-map pointer
    emitter.label("__rt_sc_merge_replace_sub");
    emitter.instruction("ldr x0, [sp, #48]");                                   // pass the current option map as the left input
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass the incoming option map as the right input
    emitter.instruction("bl __rt_array_replace");                               // shallow right-wins merge exactly one option level
    emitter.instruction("b __rt_sc_merge_insert_sub");                          // insert the newly owned merged option map
    emitter.label("__rt_sc_merge_clone_sub");
    emitter.instruction("ldr x0, [sp, #40]");                                   // pass the incoming option map to the COW clone helper
    emitter.instruction("bl __rt_hash_clone_shallow");                          // isolate a new wrapper's option map from caller mutation
    emitter.label("__rt_sc_merge_insert_sub");
    emitter.instruction("mov x3, x0");                                          // transfer the owned option map as the hash value
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the owned result wrapper map
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the wrapper key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the wrapper key length
    emitter.instruction("mov x4, xzr");                                         // associative maps use no high payload word
    emitter.instruction("mov x5, #5");                                          // runtime tag 5 identifies an associative array
    emitter.instruction("bl __rt_hash_set");                                    // replace or append this wrapper's merged option map
    emitter.instruction("str x0, [sp, #8]");                                    // preserve a possibly relocated result wrapper map
    emitter.instruction("b __rt_sc_merge_loop");                                // merge the next incoming wrapper
    emitter.label("__rt_sc_merge_clone_incoming");
    emitter.instruction("mov x0, x1");                                          // clone the incoming map when no existing options exist
    emitter.instruction("bl __rt_hash_clone_shallow");                          // return a caller-independent COW root
    emitter.instruction("str x0, [sp, #8]");                                    // share the ordinary return path
    emitter.label("__rt_sc_merge_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the owned merged wrapper map
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore the caller frame and link register
    emitter.instruction("add sp, sp, #128");                                    // release merge iteration state
    emitter.instruction("ret");                                                 // return to stream-context lowering
}

/// Emits the Linux x86_64 wrapper-to-options merge implementation.
fn emit_stream_context_merge_options_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: merge stream-context wrapper options ---");
    emitter.label_global("__rt_stream_context_merge_options");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 96");                                         // reserve aligned merge iteration state
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the incoming wrapper map
    emitter.instruction("test rdi, rdi");                                       // does an existing wrapper map need cloning?
    emitter.instruction("jz __rt_sc_merge_clone_incoming");                     // an empty context starts as an incoming clone
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone the existing wrapper map for COW mutation
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the owned result wrapper map
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize incoming-wrapper iteration
    emitter.label("__rt_sc_merge_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the incoming wrapper map
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload its insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch wrapper key and option-map payload
    emitter.instruction("cmp rax, -1");                                         // did the iterator reach its end sentinel?
    emitter.instruction("je __rt_sc_merge_done");                               // finish after every incoming wrapper
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the next incoming-wrapper cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // preserve the wrapper key pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the wrapper key length
    emitter.instruction("cmp r9, 5");                                           // is the incoming wrapper value an associative option map?
    emitter.instruction("je __rt_sc_merge_incoming_direct");                    // direct hash payloads already expose their pointer
    emitter.instruction("cmp r9, 7");                                           // is the wrapper value a boxed Mixed payload?
    emitter.instruction("jne __rt_sc_merge_loop");                              // ignore malformed non-array wrapper values
    emitter.instruction("mov rax, rcx");                                        // pass the boxed wrapper value to Mixed unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose its concrete tag and payload words
    emitter.instruction("cmp rax, 5");                                          // did the boxed value contain an associative option map?
    emitter.instruction("jne __rt_sc_merge_loop");                              // ignore malformed boxed wrapper values
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve the unboxed incoming option-map pointer
    emitter.instruction("jmp __rt_sc_merge_find_existing");                     // continue with existing-wrapper lookup
    emitter.label("__rt_sc_merge_incoming_direct");
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // preserve the direct incoming option-map pointer
    emitter.label("__rt_sc_merge_find_existing");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the result wrapper map for lookup
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the wrapper key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the wrapper key length
    emitter.instruction("call __rt_hash_get");                                  // find the current option map for this wrapper
    emitter.instruction("test rax, rax");                                       // was the wrapper already present?
    emitter.instruction("jz __rt_sc_merge_clone_sub");                          // a new wrapper starts as a clone of its incoming map
    emitter.instruction("cmp rcx, 5");                                          // is the current wrapper value a direct associative map?
    emitter.instruction("je __rt_sc_merge_existing_direct");                    // use its direct payload pointer
    emitter.instruction("cmp rcx, 7");                                          // is the current wrapper value boxed as Mixed?
    emitter.instruction("jne __rt_sc_merge_clone_sub");                         // malformed current values are replaced by the incoming map
    emitter.instruction("mov rax, rdi");                                        // pass the boxed current option map to Mixed unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose its concrete tag and payload words
    emitter.instruction("cmp rax, 5");                                          // did the box contain an associative option map?
    emitter.instruction("jne __rt_sc_merge_clone_sub");                         // malformed current values are replaced
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the unboxed current option-map pointer
    emitter.instruction("jmp __rt_sc_merge_replace_sub");                       // shallow-merge the two option maps
    emitter.label("__rt_sc_merge_existing_direct");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the direct current option-map pointer
    emitter.label("__rt_sc_merge_replace_sub");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // pass the current option map as the left input
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass the incoming option map as the right input
    emitter.instruction("call __rt_array_replace");                             // shallow right-wins merge exactly one option level
    emitter.instruction("jmp __rt_sc_merge_insert_sub");                        // insert the newly owned merged option map
    emitter.label("__rt_sc_merge_clone_sub");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // pass the incoming option map to the COW clone helper
    emitter.instruction("call __rt_hash_clone_shallow");                        // isolate a new wrapper's option map from caller mutation
    emitter.label("__rt_sc_merge_insert_sub");
    emitter.instruction("mov rcx, rax");                                        // transfer the owned option map as the hash value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the owned result wrapper map
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the wrapper key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the wrapper key length
    emitter.instruction("xor r8, r8");                                          // associative maps use no high payload word
    emitter.instruction("mov r9, 5");                                           // runtime tag 5 identifies an associative array
    emitter.instruction("call __rt_hash_set");                                  // replace or append this wrapper's merged option map
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve a possibly relocated result wrapper map
    emitter.instruction("jmp __rt_sc_merge_loop");                              // merge the next incoming wrapper
    emitter.label("__rt_sc_merge_clone_incoming");
    emitter.instruction("mov rdi, rsi");                                        // clone the incoming map when no existing options exist
    emitter.instruction("call __rt_hash_clone_shallow");                        // return a caller-independent COW root
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // share the ordinary return path
    emitter.label("__rt_sc_merge_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the owned merged wrapper map
    emitter.instruction("add rsp, 96");                                         // release merge iteration state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to stream-context lowering
}
