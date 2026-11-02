//! Purpose:
//! Emits one-value prepend support for associative arrays with boxed gradual values.
//! Rebuilds insertion order while preserving string keys and renumbering integer keys.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_managed_runtime()`.
//!
//! Key details:
//! - The helper consumes the caller's source-hash owner and the owned boxed value.
//! - Rebuilding through `__rt_hash_spread` preserves copy-on-write aliases and PHP key rules.

use crate::codegen_support::callable_invoker_args::{
    ARRAY_GLOBAL_REF_CELL_TAG, ARRAY_LOCAL_REF_CELL_TAG, INVOKER_ARG_REF_CELL_TAG,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the associative-array one-value prepend helper for the selected target.
pub fn emit_hash_unshift_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 associative-array prepend helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_unshift_mixed ---");
    emitter.label_global("__rt_hash_unshift_mixed");
    // Frame slots: unique source=0, prepended box=8, destination=16,
    // release-wrapper flag=24, saved fp/lr=32/40.
    emitter.instruction("sub sp, sp, #48");                                     // reserve rebuild state and an aligned nested-call frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the runtime helper frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the owned boxed value to prepend
    emitter.instruction("bl __rt_hash_ensure_unique");                          // isolate this mutating owner from copy-on-write aliases
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the unique source hash to consume later
    emitter.instruction("ldr x9, [x0, #8]");                                    // reuse the source capacity as the rebuild baseline
    emitter.instruction("add x9, x9, #1");                                      // reserve at least one additional destination slot
    emitter.instruction("cmp x9, #4");                                          // tiny hashes still need practical probing capacity
    emitter.instruction("mov x10, #4");                                         // materialize the minimum destination capacity
    emitter.instruction("csel x0, x9, x10, ge");                                // clamp the destination capacity to at least four
    emitter.instruction("mov x1, #7");                                          // the destination accepts heterogeneous per-entry runtime tags
    emitter.instruction("bl __rt_hash_new");                                    // allocate the independent rebuilt hash
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the destination across insertion calls
    emitter.instruction("mov x1, #0");                                          // the prepended value receives integer key zero
    emitter.instruction("mov x2, #-1");                                         // mark the destination key as an integer
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the owned prepended Mixed cell
    emitter.instruction("ldr x5, [x9]");                                        // inspect its PHP-visible runtime value tag
    emitter.instruction(&format!("cmp x5, #{}", INVOKER_ARG_REF_CELL_TAG));     // borrowed reference markers must retain their wrapper identity
    emitter.instruction("b.eq __rt_hash_unshift_mixed_aarch64_marker");         // store the marker cell itself instead of its internal fields
    emitter.instruction(&format!("cmp x5, #{}", ARRAY_GLOBAL_REF_CELL_TAG));    // request-global reference markers also carry writable identity
    emitter.instruction("b.eq __rt_hash_unshift_mixed_aarch64_marker");         // keep the owning marker wrapper in the rebuilt array
    emitter.instruction(&format!("cmp x5, #{}", ARRAY_LOCAL_REF_CELL_TAG));     // promoted local reference markers share the same contract
    emitter.instruction("b.eq __rt_hash_unshift_mixed_aarch64_marker");         // preserve the local reference marker wrapper too
    emitter.instruction("ldr x3, [x9, #8]");                                    // transfer the ordinary cell's low payload word
    emitter.instruction("ldr x4, [x9, #16]");                                   // transfer the ordinary cell's high payload word
    emitter.instruction("mov x10, #1");                                         // remember to release only the transferred wrapper after insertion
    emitter.instruction("str x10, [sp, #24]");                                  // preserve the wrapper-release decision across hash_set
    emitter.instruction("b __rt_hash_unshift_mixed_aarch64_insert");            // insert the concrete per-entry runtime representation
    emitter.label("__rt_hash_unshift_mixed_aarch64_marker");
    emitter.instruction("mov x3, x9");                                          // transfer the reference marker's boxed cell pointer
    emitter.instruction("mov x4, #0");                                          // boxed marker entries use no high payload word
    emitter.instruction("mov x5, #7");                                          // hash tag 7 identifies a boxed Mixed marker cell
    emitter.instruction("str xzr, [sp, #24]");                                  // the destination owns the marker wrapper after insertion
    emitter.label("__rt_hash_unshift_mixed_aarch64_insert");
    emitter.instruction("bl __rt_hash_set");                                    // insert the prepended value as the first entry
    emitter.instruction("str x0, [sp, #16]");                                   // preserve a possibly grown destination hash
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload whether the ordinary Mixed wrapper was transferred
    emitter.instruction("cbz x10, __rt_hash_unshift_mixed_aarch64_wrapper_done"); // marker wrappers remain owned by their destination bucket
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the now-empty ordinary Mixed wrapper
    emitter.instruction("bl __rt_heap_free");                                   // free only the wrapper while leaving its transferred payload alive
    emitter.label("__rt_hash_unshift_mixed_aarch64_wrapper_done");
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the unique source hash as spread input two
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the destination hash as spread input one
    emitter.instruction("bl __rt_hash_spread");                                 // append source entries with PHP integer-key renumbering
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the completed destination across source release
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the consumed unique source owner
    emitter.instruction("bl __rt_decref_hash");                                 // release the source after every entry was retained into destination
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the independently owned rebuilt hash
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the rebuild helper frame
    emitter.instruction("ret");                                                 // return the prepended associative array
}

/// Emits the x86_64 associative-array prepend helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_unshift_mixed ---");
    emitter.label_global("__rt_hash_unshift_mixed");
    // Frame slots: unique source=-8, prepended box=-16, destination=-24,
    // release-wrapper flag=-32.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve aligned rebuild state
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the owned boxed value to prepend
    emitter.instruction("call __rt_hash_ensure_unique");                        // isolate this mutating owner from copy-on-write aliases
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the unique source hash to consume later
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // reuse source capacity as the rebuild baseline
    emitter.instruction("add rdi, 1");                                          // reserve at least one additional destination slot
    emitter.instruction("cmp rdi, 4");                                          // tiny hashes still need practical probing capacity
    emitter.instruction("mov r10, 4");                                          // materialize the minimum destination capacity
    emitter.instruction("cmovl rdi, r10");                                      // clamp destination capacity to at least four
    emitter.instruction("mov rsi, 7");                                          // the destination accepts heterogeneous per-entry runtime tags
    emitter.instruction("call __rt_hash_new");                                  // allocate the independent rebuilt hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve destination across insertion calls
    emitter.instruction("mov rdi, rax");                                        // pass destination hash to the setter
    emitter.instruction("xor esi, esi");                                        // the prepended value receives integer key zero
    emitter.instruction("mov rdx, -1");                                         // mark the destination key as an integer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // load the owned prepended Mixed cell
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // inspect its PHP-visible runtime value tag
    emitter.instruction(&format!("cmp r9, {}", INVOKER_ARG_REF_CELL_TAG));      // borrowed reference markers must retain their wrapper identity
    emitter.instruction("je __rt_hash_unshift_mixed_x86_marker");               // store the marker cell itself instead of its internal fields
    emitter.instruction(&format!("cmp r9, {}", ARRAY_GLOBAL_REF_CELL_TAG));     // request-global reference markers also carry writable identity
    emitter.instruction("je __rt_hash_unshift_mixed_x86_marker");               // keep the owning marker wrapper in the rebuilt array
    emitter.instruction(&format!("cmp r9, {}", ARRAY_LOCAL_REF_CELL_TAG));      // promoted local reference markers share the same contract
    emitter.instruction("je __rt_hash_unshift_mixed_x86_marker");               // preserve the local reference marker wrapper too
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // transfer the ordinary cell's low payload word
    emitter.instruction("mov r8, QWORD PTR [r10 + 16]");                        // transfer the ordinary cell's high payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // remember to release only the transferred wrapper after insertion
    emitter.instruction("jmp __rt_hash_unshift_mixed_x86_insert");              // insert the concrete per-entry runtime representation
    emitter.label("__rt_hash_unshift_mixed_x86_marker");
    emitter.instruction("mov rcx, r10");                                        // transfer the reference marker's boxed cell pointer
    emitter.instruction("xor r8d, r8d");                                        // boxed marker entries use no high payload word
    emitter.instruction("mov r9, 7");                                           // hash tag 7 identifies a boxed Mixed marker cell
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // the destination owns the marker wrapper after insertion
    emitter.label("__rt_hash_unshift_mixed_x86_insert");
    emitter.instruction("call __rt_hash_set");                                  // insert the prepended value as the first entry
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve a possibly grown destination hash
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // was an ordinary wrapper emptied into the destination entry?
    emitter.instruction("je __rt_hash_unshift_mixed_x86_wrapper_done");         // marker wrappers remain owned by their destination bucket
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the now-empty ordinary Mixed wrapper
    emitter.instruction("call __rt_heap_free");                                 // free only the wrapper while leaving its transferred payload alive
    emitter.label("__rt_hash_unshift_mixed_x86_wrapper_done");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload destination hash as spread input one after optional wrapper release
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // pass unique source hash as spread input two
    emitter.instruction("call __rt_hash_spread");                               // append source entries with PHP integer-key renumbering
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve completed destination across source release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the consumed unique source owner
    emitter.instruction("call __rt_decref_hash");                               // release source after every entry was retained into destination
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the independently owned rebuilt hash
    emitter.instruction("add rsp, 32");                                         // release rebuild spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the prepended associative array
}
