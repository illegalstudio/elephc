//! Purpose:
//! Allocates zend_string storage and retires owned zval/string/array trees.
//!
//! Called from:
//! - Compiler zval interoperability adapters.
//!
//! Key details:
//! - Standalone zval blocks own their string/array children; bucket cells own only their children.
//! - Object and resource payloads remain borrowed native pointers and are never released here.
//! - PHP value conversion and pack/unpack orchestration stay in the compiler.

use crate::{Arch, Emitter};

/// zval_string_new: allocate a zend_string and copy bytes from an elephc string.
/// Input:  x1 = source string pointer, x2 = source length
/// Output: x0 = zend_string pointer (val bytes start at offset 24)
pub fn emit_zval_string_new(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_zval_string_new_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_string_new ---");
    emitter.label_global("__rt_zval_string_new");

    // -- set up stack frame and spill the source pointer and length --
    emitter.instruction("sub sp, sp, #48");                                     // reserve source/length/zend_str slots plus frame records
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the source string pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the source string length

    // -- allocate the zend_string storage (24 header + len + 1 NUL, aligned to 8) --
    emitter.instruction("add x0, x2, #25");                                     // header(24) + length + NUL byte
    emitter.instruction("add x0, x0, #7");                                      // round up to an 8-byte multiple
    emitter.instruction("bic x0, x0, #7");                                      // align the allocation size down to 8 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = zend_string storage pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the zend_string pointer

    // -- fill the zend_string header --
    emitter.instruction("mov x9, #1");                                          // refcount starts at one owner
    emitter.instruction("str w9, [x0]");                                        // store refcount at offset 0 (32-bit)
    emitter.instruction("mov x9, #6");                                          // gc type_info = IS_STRING (6)
    emitter.instruction("str w9, [x0, #4]");                                    // store gc type_info at offset 4 (32-bit)
    emitter.instruction("str xzr, [x0, #8]");                                   // zero the hash slot h at offset 8
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the source length
    emitter.instruction("str x9, [x0, #16]");                                   // store len at offset 16

    // -- copy source bytes into zend_string val[] at offset 24 --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source string pointer
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the zend_string pointer
    emitter.instruction("add x0, x0, #24");                                     // dst = zend_string val[] base
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the byte count
    emitter.instruction("cbz x2, __rt_zval_string_new_done_copy");              // skip the copy loop for empty strings

    emitter.label("__rt_zval_string_new_copy_loop");
    emitter.instruction("ldrb w9, [x1]");                                       // read the next source byte
    emitter.instruction("strb w9, [x0]");                                       // write the byte into the zend_string val[]
    emitter.instruction("add x1, x1, #1");                                      // advance the source cursor
    emitter.instruction("add x0, x0, #1");                                      // advance the destination cursor
    emitter.instruction("subs x2, x2, #1");                                     // decrement the remaining byte count
    emitter.instruction("b.ne __rt_zval_string_new_copy_loop");                 // loop until every byte is copied

    emitter.label("__rt_zval_string_new_done_copy");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the zend_string pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the length for the NUL offset
    emitter.instruction("add x3, x0, x2");                                      // compute the NUL address base (zend_str + len)
    emitter.instruction("strb wzr, [x3, #24]");                                 // write the NUL terminator at zend_str + 24 + len
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return the zend_string pointer in x0
}

/// x86_64 Linux implementation of `__rt_zval_string_new`.
/// Input:  rax = source string pointer, rdx = source length
/// Output: rax = zend_string pointer (val bytes start at offset 24)
fn emit_zval_string_new_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_string_new ---");
    emitter.label_global("__rt_zval_string_new");

    // -- set up stack frame and spill the source pointer and length --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve source/length/zend_str slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length

    // -- allocate the zend_string storage (24 header + len + 1 NUL, aligned to 8) --
    emitter.instruction("lea rax, [rdx + 25]");                                 // header(24) + length + NUL byte
    emitter.instruction("add rax, 7");                                          // round up to an 8-byte multiple
    emitter.instruction("and rax, -8");                                         // align the allocation size down to 8 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = zend_string storage pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the zend_string pointer

    // -- fill the zend_string header --
    emitter.instruction("mov DWORD PTR [rax], 1");                              // refcount starts at one owner (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 4], 6");                          // gc type_info = IS_STRING (6)
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // zero the hash slot h at offset 8
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length
    emitter.instruction("mov QWORD PTR [rax + 16], rdx");                       // store len at offset 16

    // -- copy source bytes into zend_string val[] at offset 24 --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source string pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the zend_string pointer
    emitter.instruction("add rdi, 24");                                         // dst = zend_string val[] base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the byte count
    emitter.instruction("test rcx, rcx");                                       // check for an empty string
    emitter.instruction("je __rt_zval_string_new_done_copy");                   // skip the copy loop for empty strings

    emitter.label("__rt_zval_string_new_copy_loop");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // read the next source byte
    emitter.instruction("mov BYTE PTR [rdi], al");                              // write the byte into the zend_string val[]
    emitter.instruction("inc rsi");                                             // advance the source cursor
    emitter.instruction("inc rdi");                                             // advance the destination cursor
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count
    emitter.instruction("jne __rt_zval_string_new_copy_loop");                  // loop until every byte is copied

    emitter.label("__rt_zval_string_new_done_copy");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the zend_string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length for the NUL offset
    emitter.instruction("mov byte ptr [rax + rcx + 24], 0");                    // write the NUL terminator at zend_str + 24 + len
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the zend_string pointer in rax
    emitter.instruction("add rsp, 32");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the zend_string pointer in rax
}

/// zval_free: release a zval and its owned PHP-shaped children.
/// Input:  x0 / rax = zval pointer
/// Output: none
pub fn emit_zval_free(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_zval_free_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_free ---");
    emitter.label_global("__rt_zval_free");

    // -- save the link register and spill the zval pointer across the children free --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve frame and link registers (32 keeps 16-byte alignment for the spill slot)
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the helper
    emitter.instruction("str x0, [sp, #16]");                                   // spill the zval pointer (caller-saved regs do not survive the nested heap_free inside free_children)
    emitter.instruction("bl __rt_zval_free_children");                          // release owned string/array children
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the zval pointer after the children free clobbered the scratch registers
    emitter.instruction("bl __rt_heap_free");                                   // release the zval storage block
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame and link registers
    emitter.instruction("ret");                                                 // return to the caller

    emit_zval_free_children_aarch64(emitter);
}

/// zval_free_children: release owned children referenced by a zval value.
/// Input:  x0 = zval pointer
/// Output: none (the zval block itself is not freed)
fn emit_zval_free_children_aarch64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_free_children ---");
    emitter.label_global("__rt_zval_free_children");

    // -- save the link register across child free calls --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve frame and link registers
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the helper
    emitter.instruction("mov x10, x0");                                         // keep the zval pointer across child calls
    emitter.instruction("ldrb w9, [x10, #8]");                                  // load the zval type byte
    emitter.instruction("cmp w9, #6");                                          // IS_STRING owns a zend_string
    emitter.instruction("b.eq __rt_zval_free_children_string");                 // take the string-free path when the value is a zend_string
    emitter.instruction("cmp w9, #7");                                          // IS_ARRAY owns a zend_array
    emitter.instruction("b.eq __rt_zval_free_children_array");                  // take the array-free path when the value is a zend_array
    emitter.instruction("b __rt_zval_free_children_done");                      // scalars/objects/resources own nothing extra

    emitter.label("__rt_zval_free_children_string");
    emitter.instruction("ldr x0, [x10]");                                       // load the zend_string pointer from the zval value
    emitter.instruction("bl __rt_heap_free");                                   // release the zend_string storage
    emitter.instruction("b __rt_zval_free_children_done");                      // done once the zend_string is released

    emitter.label("__rt_zval_free_children_array");
    emitter.instruction("ldr x0, [x10]");                                       // load the zend_array pointer from the zval value
    emitter.instruction("bl __rt_zval_free_array");                             // release the zend_array and its element tree
    emitter.instruction("b __rt_zval_free_children_done");                      // done once the zend_array is released

    emitter.label("__rt_zval_free_children_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame and link registers
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 Linux implementation of `__rt_zval_free` and `__rt_zval_free_children`.
fn emit_zval_free_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_free ---");
    emitter.label_global("__rt_zval_free");

    // -- save the frame pointer and spill the zval pointer across the children free --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base for the helper
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned spill slot for the zval pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // spill the zval pointer (caller-saved regs do not survive the nested heap_free inside free_children)
    emitter.instruction("call __rt_zval_free_children");                        // release owned string/array children
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the zval pointer after the children free clobbered the scratch registers
    emitter.instruction("call __rt_heap_free");                                 // release the zval storage block
    emitter.instruction("add rsp, 16");                                         // release the spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.blank();
    emitter.comment("--- runtime: zval_free_children ---");
    emitter.label_global("__rt_zval_free_children");

    // -- save the frame pointer across child free calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base for the helper
    emitter.instruction("mov r10, rax");                                        // keep the zval pointer across child calls
    emitter.instruction("movzx rcx, BYTE PTR [r10 + 8]");                       // load the zval type byte
    emitter.instruction("cmp rcx, 6");                                          // IS_STRING owns a zend_string
    emitter.instruction("je __rt_zval_free_children_string");                   // take the string-free path when the value is a zend_string
    emitter.instruction("cmp rcx, 7");                                          // IS_ARRAY owns a zend_array
    emitter.instruction("je __rt_zval_free_children_array");                    // take the array-free path when the value is a zend_array
    emitter.instruction("jmp __rt_zval_free_children_done");                    // scalars/objects/resources own nothing extra

    emitter.label("__rt_zval_free_children_string");
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the zend_string pointer from the zval value
    emitter.instruction("call __rt_heap_free");                                 // release the zend_string storage
    emitter.instruction("jmp __rt_zval_free_children_done");                    // done once the zend_string is released

    emitter.label("__rt_zval_free_children_array");
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the zend_array pointer from the zval value
    emitter.instruction("call __rt_zval_free_array");                           // release the zend_array and its element tree
    emitter.instruction("jmp __rt_zval_free_children_done");                    // done once the zend_array is released

    emitter.label("__rt_zval_free_children_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// zval_free_array: release a zend_array, its element tree, and its data block.
/// Input:  x0 / rax = zend_array (HashTable) pointer
/// Output: none
pub fn emit_zval_free_array(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_zval_free_array_linux_x86_64(emitter);
        return;
    }
    emit_zval_free_array_aarch64(emitter);
}

/// ARM64 implementation of `__rt_zval_free_array`.
fn emit_zval_free_array_aarch64(emitter: &mut impl Emitter) {
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
fn emit_zval_free_array_linux_x86_64(emitter: &mut impl Emitter) {
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
