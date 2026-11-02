//! Purpose:
//! Emits dynamic `ksort` and `krsort` dispatchers for boxed `Mixed` array values.
//! Keeps copy-on-write pointer replacement inside the owning Mixed cell.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime()`.
//!
//! Key details:
//! - Runtime tag 5 is a hash-backed associative array and must be split before relinking.
//! - Runtime tag 4 is indexed storage; ascending key sort is already ordered, while the current
//!   indexed representation cannot encode descending integer keys and therefore remains unchanged.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the boxed-Mixed `ksort` and `krsort` runtime entry points.
///
/// Each helper accepts a Mixed cell pointer in the first integer argument register. Hash payloads
/// are copy-on-write split, written back to that cell, and then reordered by the concrete hash
/// sorter. Non-hash payloads return unchanged.
pub fn emit_ksort(emitter: &mut Emitter) {
    emit_mixed_key_sort(emitter, "__rt_ksort", "__rt_hash_ksort", "ascending");
    emit_mixed_key_sort(emitter, "__rt_krsort", "__rt_hash_krsort", "descending");
}

/// Emits one target-specific boxed-Mixed key-sort dispatcher.
fn emit_mixed_key_sort(
    emitter: &mut Emitter,
    entry: &str,
    hash_helper: &str,
    direction: &str,
) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: Mixed key sort ({direction}) ---"));
    emitter.label_global(entry);
    let done = format!("{entry}_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {done}"));                    // null Mixed cell pointers leave the receiver unchanged
            emitter.instruction("ldr x9, [x0]");                                // load the runtime tag from the boxed Mixed cell
            emitter.instruction("cmp x9, #5");                                  // runtime tag 5 identifies a hash-backed associative array
            emitter.instruction(&format!("b.ne {done}"));                       // indexed and non-array values need no hash relinking
            emitter.instruction("sub sp, sp, #32");                             // reserve an aligned spill frame for the owning Mixed cell
            emitter.instruction("stp x29, x30, [sp, #16]");                     // preserve the caller frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame pointer
            emitter.instruction("str x0, [sp, #0]");                            // preserve the owning Mixed cell across copy-on-write work
            emitter.instruction("ldr x0, [x0, #8]");                            // pass the hash payload to the copy-on-write splitter
            emitter.instruction("bl __rt_hash_ensure_unique");                  // return uniquely owned hash storage in x0
            emitter.instruction("ldr x9, [sp, #0]");                            // reload the owning Mixed cell pointer
            emitter.instruction("str x0, [x9, #8]");                            // publish a relocated copy-on-write payload in the Mixed cell
            emitter.instruction(&format!("bl {hash_helper}"));                  // relink the unique hash table in the requested key order
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore the caller frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the aligned helper frame
            emitter.label(&done);
            emitter.instruction("ret");                                         // return after mutating the receiver in place
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // reject a null Mixed cell pointer before reading its tag
            emitter.instruction(&format!("je {done}"));                         // null receivers remain unchanged
            emitter.instruction("cmp QWORD PTR [rdi], 5");                      // runtime tag 5 identifies a hash-backed associative array
            emitter.instruction(&format!("jne {done}"));                        // indexed and non-array values need no hash relinking
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish an aligned frame for nested runtime calls
            emitter.instruction("sub rsp, 16");                                 // reserve a spill slot for the owning Mixed cell
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // preserve the owning Mixed cell across copy-on-write work
            emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                // pass the hash payload to the copy-on-write splitter
            emitter.instruction("call __rt_hash_ensure_unique");                // return uniquely owned hash storage in rax
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the owning Mixed cell pointer
            emitter.instruction("mov QWORD PTR [r10 + 8], rax");                // publish a relocated copy-on-write payload in the Mixed cell
            emitter.instruction("mov rdi, rax");                                // pass the unique hash pointer to the concrete sorter
            emitter.instruction(&format!("call {hash_helper}"));                // relink the hash table in the requested key order
            emitter.instruction("add rsp, 16");                                 // release the Mixed-cell spill slot
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.label(&done);
            emitter.instruction("ret");                                         // return after mutating the receiver in place
        }
    }
}
