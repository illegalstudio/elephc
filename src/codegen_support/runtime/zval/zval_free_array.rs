//! Purpose:
//! Emits the `__rt_zval_free_array` runtime helper that releases a PHP
//! `zend_array` (HashTable), its element tree, and its data block.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and from `__rt_zval_free_children` when freeing nested arrays.
//!
//! Key details:
//! - Walks the element tree (packed or hash) via `__rt_zval_free_children`,
//!   frees each bucket's `zend_string` key when present, then frees the data
//!   block (base = `arData + (nTableMask << 2)`, which covers both the packed
//!   sentinel prefix and the hash index prefix) and the HashTable itself.
//! - The empty-hash placeholder has `arData = NULL`; the element walk is then
//!   skipped and only the HashTable structure is freed.
//! - Emits both ARM64 and x86_64 variants gated on `emitter.target.arch`.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_free_array: release a zend_array, its element tree, and its data block.
/// Input:  x0 / rax = zend_array (HashTable) pointer
/// Output: none
pub fn emit_zval_free_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_free_array_linux_x86_64(emitter);
        return;
    }
    emit_zval_free_array_aarch64(emitter);
}

/// ARM64 implementation of `__rt_zval_free_array`.
fn emit_zval_free_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_free_array ---");
    emitter.label_global("__rt_zval_free_array");

    // -- set up a frame and stash the HashTable pointer --
    emitter.instruction("sub sp, sp, #48");                                     // reserve ht/i/arData/nTableMask slots plus frame records
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the HashTable pointer
    emitter.instruction("str xzr, [sp, #8]");                                   // initialize the loop index i = 0

    // -- walk the element tree when arData is present --
    emitter.instruction("ldr x9, [x0, #16]");                                   // load arData
    emitter.instruction("cbz x9, __rt_zval_free_array_no_data");                // empty hash placeholder has arData = NULL

    emitter.label("__rt_zval_free_array_loop");
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the HashTable pointer
    emitter.instruction("ldr w1, [x1, #24]");                                   // load nNumUsed (32-bit, zero-extended)
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload i
    emitter.instruction("cmp x0, x1");                                          // has every used bucket been freed?
    emitter.instruction("b.ge __rt_zval_free_array_loop_done");                 // exit once every used bucket is freed
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the HashTable pointer
    emitter.instruction("ldr x9, [x9, #16]");                                   // reload arData
    emitter.instruction("lsl x10, x0, #5");                                     // i * 32 bytes per bucket
    emitter.instruction("add x0, x9, x10");                                     // x0 = bucket address (= zval pointer)
    emitter.instruction("bl __rt_zval_free_children");                          // release the bucket's owned string/array children

    // -- free the bucket string key (NULL for packed/int-key buckets; heap_free is null-safe) --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the HashTable pointer
    emitter.instruction("ldr x9, [x9, #16]");                                   // reload arData
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload i
    emitter.instruction("lsl x11, x10, #5");                                    // i * 32 bytes per bucket
    emitter.instruction("add x9, x9, x11");                                     // x9 = bucket address
    emitter.instruction("ldr x0, [x9, #24]");                                   // load the bucket key pointer (NULL when none)
    emitter.instruction("bl __rt_heap_free");                                   // release the zend_string key (null-safe no-op when NULL)

    emitter.instruction("ldr x9, [sp, #8]");                                    // reload i
    emitter.instruction("add x9, x9, #1");                                      // advance to the next bucket
    emitter.instruction("str x9, [sp, #8]");                                    // store the incremented index
    emitter.instruction("b __rt_zval_free_array_loop");                         // continue freeing buckets

    emitter.label("__rt_zval_free_array_loop_done");
    // -- free the data block: base = arData + (nTableMask << 2) (works for packed and hash) --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the HashTable pointer
    emitter.instruction("ldr w10, [x9, #12]");                                  // load nTableMask (32-bit)
    emitter.instruction("sxtw x10, w10");                                       // sign-extend nTableMask to 64-bit
    emitter.instruction("lsl x10, x10, #2");                                    // nTableMask << 2 (negative offset to the base)
    emitter.instruction("ldr x0, [x9, #16]");                                   // load arData
    emitter.instruction("add x0, x0, x10");                                     // base = arData + (nTableMask << 2)
    emitter.instruction("bl __rt_heap_free");                                   // release the bucket + hash-index storage block

    emitter.label("__rt_zval_free_array_no_data");
    // -- free the HashTable structure itself --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the HashTable pointer
    emitter.instruction("bl __rt_heap_free");                                   // release the HashTable storage
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 Linux implementation of `__rt_zval_free_array`.
fn emit_zval_free_array_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_free_array ---");
    emitter.label_global("__rt_zval_free_array");

    // -- set up a frame and stash the HashTable pointer --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve ht/i slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the HashTable pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // initialize the loop index i = 0

    // -- walk the element tree when arData is present --
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // load arData
    emitter.instruction("test rcx, rcx");                                       // is arData NULL?
    emitter.instruction("jz __rt_zval_free_array_no_data");                     // empty hash placeholder has arData = NULL

    emitter.label("__rt_zval_free_array_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the HashTable pointer
    emitter.instruction("mov ecx, DWORD PTR [rcx + 24]");                       // load nNumUsed (32-bit)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload i
    emitter.instruction("cmp rax, rcx");                                        // has every used bucket been freed?
    emitter.instruction("jge __rt_zval_free_array_loop_done");                  // exit once every used bucket is freed
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the HashTable pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 16]");                         // reload arData
    emitter.instruction("mov r10, rax");                                        // copy i for the bucket offset
    emitter.instruction("shl r10, 5");                                          // i * 32 bytes per bucket
    emitter.instruction("lea rax, [r9 + r10]");                                 // rax = bucket address (= zval pointer)
    emitter.instruction("call __rt_zval_free_children");                        // release the bucket's owned string/array children

    // -- free the bucket string key (NULL for packed/int-key buckets; heap_free is null-safe) --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the HashTable pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + 16]");                         // reload arData
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload i
    emitter.instruction("mov r11, r10");                                        // copy i for the bucket offset
    emitter.instruction("shl r11, 5");                                          // i * 32 bytes per bucket
    emitter.instruction("lea r9, [r9 + r11]");                                  // r9 = bucket address
    emitter.instruction("mov rax, QWORD PTR [r9 + 24]");                        // load the bucket key pointer (NULL when none)
    emitter.instruction("call __rt_heap_free");                                 // release the zend_string key (null-safe no-op when NULL)

    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload i
    emitter.instruction("inc r9");                                              // advance to the next bucket
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // store the incremented index
    emitter.instruction("jmp __rt_zval_free_array_loop");                       // continue freeing buckets

    emitter.label("__rt_zval_free_array_loop_done");
    // -- free the data block: base = arData + (nTableMask << 2) (works for packed and hash) --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the HashTable pointer
    emitter.instruction("movsxd r10, DWORD PTR [r9 + 12]");                     // sign-extend nTableMask (32-bit) to 64-bit
    emitter.instruction("shl r10, 2");                                          // nTableMask << 2 (negative offset to the base)
    emitter.instruction("mov rax, QWORD PTR [r9 + 16]");                        // load arData
    emitter.instruction("lea rax, [rax + r10]");                                // base = arData + (nTableMask << 2)
    emitter.instruction("call __rt_heap_free");                                 // release the bucket + hash-index storage block

    emitter.label("__rt_zval_free_array_no_data");
    // -- free the HashTable structure itself --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the HashTable pointer
    emitter.instruction("call __rt_heap_free");                                 // release the HashTable storage
    emitter.instruction("add rsp, 32");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
