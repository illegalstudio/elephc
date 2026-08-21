//! Purpose:
//! Emits the weak runtime cache that canonicalizes PHP wrappers for native DOM handles.
//! Keeps wrapper identity stable without retaining otherwise unreachable PHP objects.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - DOM wrapper materialization and finalization helpers.
//!
//! Key details:
//! - Entries contain context, handle, borrowed object pointer, and next pointer.
//! - Cache hits retain the returned PHP object; cache entries themselves never do.
//! - Finalization removes the exact triple before native handle release.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits target-specific weak DOM wrapper cache lookup, insert, and removal helpers.
pub(crate) fn emit_dom_wrapper_cache(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dom_wrapper_cache_x86_64(emitter);
    } else {
        emit_dom_wrapper_cache_aarch64(emitter);
    }
}

/// Emits the AArch64 weak DOM wrapper cache helpers.
fn emit_dom_wrapper_cache_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: weak DOM wrapper identity cache ---");
    emitter.label_global("__rt_dom_wrapper_cache_get");
    abi::emit_symbol_address(emitter, "x9", "_elephc_dom_wrapper_cache_head");
    emitter.instruction("ldr x9, [x9]");                                        // load the first weak cache entry
    emitter.label("__rt_dom_wrapper_cache_get_loop");
    emitter.instruction("cbz x9, __rt_dom_wrapper_cache_get_miss");             // an exhausted list has no matching wrapper
    emitter.instruction("ldr x10, [x9]");                                       // load the entry's DOM context ID
    emitter.instruction("cmp x10, x0");                                         // does the context match the lookup key?
    emitter.instruction("b.ne __rt_dom_wrapper_cache_get_next");                // skip entries owned by another context
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the entry's native handle
    emitter.instruction("cmp x10, x1");                                         // does the generation-checked handle match?
    emitter.instruction("b.ne __rt_dom_wrapper_cache_get_next");                // continue when native identity differs
    emitter.instruction("ldr x0, [x9, #16]");                                   // return the borrowed canonical PHP object pointer
    emitter.instruction("b __rt_incref");                                       // retain the object for the newly materialized result owner
    emitter.label("__rt_dom_wrapper_cache_get_next");
    emitter.instruction("ldr x9, [x9, #24]");                                   // advance to the next weak cache entry
    emitter.instruction("b __rt_dom_wrapper_cache_get_loop");                   // inspect the remaining entries
    emitter.label("__rt_dom_wrapper_cache_get_miss");
    emitter.instruction("mov x0, xzr");                                         // return null so materialization allocates a wrapper
    emitter.instruction("ret");                                                 // return the cache miss to codegen

    emitter.label_global("__rt_dom_wrapper_cache_put");
    emitter.instruction("sub sp, sp, #48");                                     // reserve key, object, and caller-frame storage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame across heap allocation
    emitter.instruction("mov x29, sp");                                         // establish the cache insertion frame
    emitter.instruction("stp x0, x1, [sp]");                                    // preserve the context and native handle
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the borrowed PHP wrapper pointer
    emitter.instruction("mov x0, #32");                                         // allocate one four-word weak cache entry
    emitter.instruction("bl __rt_heap_alloc");                                  // obtain raw runtime storage for the entry
    emitter.instruction("ldp x9, x10, [sp]");                                   // restore the cache key
    emitter.instruction("ldr x11, [sp, #16]");                                  // restore the borrowed wrapper pointer
    emitter.instruction("stp x9, x10, [x0]");                                   // store context and handle in the new entry
    emitter.instruction("str x11, [x0, #16]");                                  // store the weak object pointer without retaining it
    abi::emit_symbol_address(emitter, "x12", "_elephc_dom_wrapper_cache_head");
    emitter.instruction("ldr x13, [x12]");                                      // load the previous list head
    emitter.instruction("str x13, [x0, #24]");                                  // link the new entry to the prior head
    emitter.instruction("str x0, [x12]");                                       // publish the new weak cache entry
    emitter.instruction("mov x0, x11");                                         // return the inserted PHP wrapper pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame
    emitter.instruction("add sp, sp, #48");                                     // release insertion scratch storage
    emitter.instruction("ret");                                                 // return the canonical newly inserted wrapper

    emitter.label_global("__rt_dom_wrapper_cache_remove");
    abi::emit_symbol_address(emitter, "x9", "_elephc_dom_wrapper_cache_head");
    emitter.instruction("mov x10, x9");                                         // begin with the global head link as predecessor
    emitter.instruction("ldr x9, [x9]");                                        // load the first candidate entry
    emitter.label("__rt_dom_wrapper_cache_remove_loop");
    emitter.instruction("cbz x9, __rt_dom_wrapper_cache_remove_done");          // a missing exact triple requires no removal
    emitter.instruction("ldr x11, [x9]");                                       // load the candidate context
    emitter.instruction("cmp x11, x0");                                         // does the context match?
    emitter.instruction("b.ne __rt_dom_wrapper_cache_remove_next");             // skip entries for another context
    emitter.instruction("ldr x11, [x9, #8]");                                   // load the candidate native handle
    emitter.instruction("cmp x11, x1");                                         // does the handle match?
    emitter.instruction("b.ne __rt_dom_wrapper_cache_remove_next");             // skip another native identity
    emitter.instruction("ldr x11, [x9, #16]");                                  // load the candidate borrowed wrapper pointer
    emitter.instruction("cmp x11, x2");                                         // is this the exact wrapper being finalized?
    emitter.instruction("b.ne __rt_dom_wrapper_cache_remove_next");             // never remove a replacement wrapper entry
    emitter.instruction("ldr x11, [x9, #24]");                                  // load the successor before unlinking the entry
    emitter.instruction("str x11, [x10]");                                      // unlink the exact weak cache entry
    emitter.instruction("mov x0, x9");                                          // pass raw cache-entry storage to heap free
    emitter.instruction("b __rt_heap_free");                                    // free the entry and return directly to the caller
    emitter.label("__rt_dom_wrapper_cache_remove_next");
    emitter.instruction("add x10, x9, #24");                                    // predecessor becomes the candidate's next link
    emitter.instruction("ldr x9, [x9, #24]");                                   // advance to the next candidate
    emitter.instruction("b __rt_dom_wrapper_cache_remove_loop");                // search the remaining weak entries
    emitter.label("__rt_dom_wrapper_cache_remove_done");
    emitter.instruction("ret");                                                 // return after an idempotent cache miss
}

/// Emits the Linux x86_64 weak DOM wrapper cache helpers.
fn emit_dom_wrapper_cache_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: weak DOM wrapper identity cache ---");
    emitter.label_global("__rt_dom_wrapper_cache_get");
    abi::emit_symbol_address(emitter, "r10", "_elephc_dom_wrapper_cache_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the first weak cache entry
    emitter.label("__rt_dom_wrapper_cache_get_loop");
    emitter.instruction("test r10, r10");                                       // is there another candidate entry?
    emitter.instruction("jz __rt_dom_wrapper_cache_get_miss");                  // an exhausted list is a cache miss
    emitter.instruction("cmp QWORD PTR [r10], rdi");                            // does the context match the lookup key?
    emitter.instruction("jne __rt_dom_wrapper_cache_get_next");                 // skip entries owned by another context
    emitter.instruction("cmp QWORD PTR [r10 + 8], rsi");                        // does the native handle match the lookup key?
    emitter.instruction("jne __rt_dom_wrapper_cache_get_next");                 // continue when native identity differs
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // return the borrowed canonical PHP object pointer
    emitter.instruction("jmp __rt_incref");                                     // retain the object for the new result owner
    emitter.label("__rt_dom_wrapper_cache_get_next");
    emitter.instruction("mov r10, QWORD PTR [r10 + 24]");                       // advance to the next weak cache entry
    emitter.instruction("jmp __rt_dom_wrapper_cache_get_loop");                 // inspect the remaining entries
    emitter.label("__rt_dom_wrapper_cache_get_miss");
    emitter.instruction("xor eax, eax");                                        // return null so materialization allocates a wrapper
    emitter.instruction("ret");                                                 // return the cache miss to codegen

    emitter.label_global("__rt_dom_wrapper_cache_put");
    emitter.instruction("push rbp");                                            // preserve the caller frame and align heap allocation
    emitter.instruction("mov rbp, rsp");                                        // establish the insertion frame
    emitter.instruction("sub rsp, 32");                                         // reserve key and object scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the DOM context ID
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the native handle
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the borrowed wrapper pointer
    emitter.instruction("mov rax, 32");                                         // allocate one four-word weak cache entry
    emitter.instruction("call __rt_heap_alloc");                                // obtain raw runtime storage for the entry
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // restore the DOM context ID
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the context in the new entry
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // restore the generation-checked handle
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the native handle in the new entry
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // restore the borrowed wrapper pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the weak object pointer without retaining it
    abi::emit_symbol_address(emitter, "r11", "_elephc_dom_wrapper_cache_head");
    emitter.instruction("mov rcx, QWORD PTR [r11]");                            // load the previous list head
    emitter.instruction("mov QWORD PTR [rax + 24], rcx");                       // link the new entry to the prior head
    emitter.instruction("mov QWORD PTR [r11], rax");                            // publish the new weak cache entry
    emitter.instruction("mov rax, r10");                                        // return the inserted PHP wrapper pointer
    emitter.instruction("mov rsp, rbp");                                        // release insertion scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the canonical newly inserted wrapper

    emitter.label_global("__rt_dom_wrapper_cache_remove");
    abi::emit_symbol_address(emitter, "r10", "_elephc_dom_wrapper_cache_head");
    emitter.instruction("mov r11, r10");                                        // begin with the global head link as predecessor
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the first candidate entry
    emitter.label("__rt_dom_wrapper_cache_remove_loop");
    emitter.instruction("test r10, r10");                                       // is there another candidate entry?
    emitter.instruction("jz __rt_dom_wrapper_cache_remove_done");               // a missing exact triple requires no removal
    emitter.instruction("cmp QWORD PTR [r10], rdi");                            // does the candidate context match?
    emitter.instruction("jne __rt_dom_wrapper_cache_remove_next");              // skip entries for another context
    emitter.instruction("cmp QWORD PTR [r10 + 8], rsi");                        // does the candidate native handle match?
    emitter.instruction("jne __rt_dom_wrapper_cache_remove_next");              // skip another native identity
    emitter.instruction("cmp QWORD PTR [r10 + 16], rdx");                       // is this the exact wrapper being finalized?
    emitter.instruction("jne __rt_dom_wrapper_cache_remove_next");              // never remove a replacement wrapper entry
    emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");                       // load the successor before unlinking the entry
    emitter.instruction("mov QWORD PTR [r11], rcx");                            // unlink the exact weak cache entry
    emitter.instruction("mov rax, r10");                                        // pass raw cache-entry storage to heap free
    emitter.instruction("jmp __rt_heap_free");                                  // free the entry and return directly to the caller
    emitter.label("__rt_dom_wrapper_cache_remove_next");
    emitter.instruction("lea r11, [r10 + 24]");                                 // predecessor becomes the candidate's next link
    emitter.instruction("mov r10, QWORD PTR [r10 + 24]");                       // advance to the next candidate
    emitter.instruction("jmp __rt_dom_wrapper_cache_remove_loop");              // search the remaining weak entries
    emitter.label("__rt_dom_wrapper_cache_remove_done");
    emitter.instruction("ret");                                                 // return after an idempotent cache miss
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies the AArch64 runtime emits lookup, insertion, removal, and weak storage.
    #[test]
    fn emits_aarch64_weak_wrapper_cache() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_dom_wrapper_cache(&mut emitter);
        let output = emitter.output();
        assert!(output.contains("__rt_dom_wrapper_cache_get:"));
        assert!(output.contains("__rt_dom_wrapper_cache_put:"));
        assert!(output.contains("__rt_dom_wrapper_cache_remove:"));
        assert!(output.contains("str x11, [x0, #16]"));
        assert!(output.contains("b __rt_incref"));
    }

    /// Verifies the x86_64 runtime emits lookup, insertion, removal, and weak storage.
    #[test]
    fn emits_x86_64_weak_wrapper_cache() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_dom_wrapper_cache(&mut emitter);
        let output = emitter.output();
        assert!(output.contains("__rt_dom_wrapper_cache_get:"));
        assert!(output.contains("__rt_dom_wrapper_cache_put:"));
        assert!(output.contains("__rt_dom_wrapper_cache_remove:"));
        assert!(output.contains("mov QWORD PTR [rax + 16], r10"));
        assert!(output.contains("jmp __rt_incref"));
    }
}
