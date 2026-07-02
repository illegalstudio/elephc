//! Purpose:
//! Emits the `__rt_zval_pack_array_hash` runtime helper that builds a PHP hash
//! `zend_array` (HashTable) from an elephc associative array.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::zval`,
//!   and from `__rt_zval_pack_element` when packing nested associative arrays.
//!
//! Key details:
//! - Walks the elephc hash in insertion order via `__rt_hash_iter_next`,
//!   computes the DJBX33A hash for string keys with `__rt_zval_djbx33a`, builds
//!   the arHash collision index plus 32-byte Buckets (with `Z_NEXT` chains and
//!   `zend_string` keys), and wraps them in a 56-byte HashTable
//!   (`nTableMask = -nTableSize`, `HASH_FLAG_STATIC_KEYS`).
//! - Each entry value is converted via `__rt_zval_pack_element`; the temporary
//!   zval block is released so the bucket owns the freshly allocated children.
//! - Emits both ARM64 and x86_64 variants gated on `emitter.target.arch`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// zval_pack_array_hash: build a PHP hash HashTable from an elephc assoc array.
/// Walks the elephc hash in insertion order via `__rt_hash_iter_next`, computes the
/// DJBX33A hash for string keys, builds the arHash collision index plus 32-byte
/// Buckets, and wraps everything in a 56-byte HashTable.
/// Input:  x0 / rax = elephc assoc array (hash) pointer
/// Output: x0 / rax = zend_array (HashTable) pointer
pub fn emit_zval_pack_array_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_pack_array_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_pack_array_hash ---");
    emitter.label_global("__rt_zval_pack_array_hash");

    // -- set up a 128-byte frame and stash the elephc hash pointer --
    emitter.instruction("sub sp, sp, #128");                                    // reserve hash/count/nTableSize/nTableMask/base/arData/bucket_idx/cursor/key_lo/key_hi/zval/nIndex/h/key slots
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the elephc hash pointer
    emitter.instruction("ldr x1, [x0]");                                        // count = hash header[0] (live entry count)
    emitter.instruction("str x1, [sp, #8]");                                    // save the count (= nNumOfElements)

    // -- nTableSize = next power of two, minimum 8 --
    emitter.instruction("mov x9, #8");                                          // start at the minimum hash table size
    emitter.label("__rt_zval_pack_array_hash_pow2");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the count
    emitter.instruction("cmp x9, x1");                                          // is the candidate size already >= count?
    emitter.instruction("b.ge __rt_zval_pack_array_hash_pow2_done");            // stop doubling once the table covers the count
    emitter.instruction("lsl x9, x9, #1");                                      // double the candidate size
    emitter.instruction("b __rt_zval_pack_array_hash_pow2");                    // retry the power-of-two search
    emitter.label("__rt_zval_pack_array_hash_pow2_done");
    emitter.instruction("str x9, [sp, #16]");                                   // save nTableSize

    // -- nTableMask = -nTableSize (sign-extended to 64-bit) --
    emitter.instruction("neg x10, x9");                                         // nTableMask as a signed 64-bit value
    emitter.instruction("str x10, [sp, #24]");                                  // save nTableMask

    // -- allocate the data block: nTableSize*4 hash index bytes + nTableSize*32 bucket bytes --
    emitter.instruction("mov x0, x9");                                          // copy nTableSize for the bucket byte count
    emitter.instruction("lsl x0, x0, #5");                                      // nTableSize * 32 bytes of bucket storage
    emitter.instruction("mov x1, x9");                                          // copy nTableSize for the hash index byte count
    emitter.instruction("lsl x1, x1, #2");                                      // nTableSize * 4 bytes of hash index storage
    emitter.instruction("add x0, x0, x1");                                      // total data block size = nTableSize * 36
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = data block base (also the arHash base)
    emitter.instruction("str x0, [sp, #32]");                                   // save the data block base
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload nTableSize
    emitter.instruction("lsl x9, x9, #2");                                      // hash index size = nTableSize * 4
    emitter.instruction("add x9, x0, x9");                                      // arData = base + hash index size
    emitter.instruction("str x9, [sp, #40]");                                   // save arData

    // -- initialize every arHash slot to HT_INVALID_IDX (0xFFFFFFFF) --
    emitter.instruction("str xzr, [sp, #48]");                                  // arHash init loop index i = 0
    emitter.label("__rt_zval_pack_array_hash_inithash");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload nTableSize
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the init loop index
    emitter.instruction("cmp x10, x9");                                         // has every slot been initialized?
    emitter.instruction("b.ge __rt_zval_pack_array_hash_inithash_done");        // exit once every hash slot is initialized
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the data block base
    emitter.instruction("lsl x12, x10, #2");                                    // i * 4 bytes per hash index slot
    emitter.instruction("add x12, x11, x12");                                   // x12 = &arHash[i]
    emitter.instruction("mov w13, #-1");                                        // HT_INVALID_IDX = 0xFFFFFFFF (32-bit)
    emitter.instruction("str w13, [x12]");                                      // arHash[i] = HT_INVALID_IDX
    emitter.instruction("add x10, x10, #1");                                    // advance to the next slot
    emitter.instruction("str x10, [sp, #48]");                                  // store the incremented init index
    emitter.instruction("b __rt_zval_pack_array_hash_inithash");                // continue the hash-index init loop

    // -- entry loop: iterate the elephc hash in insertion order --
    emitter.label("__rt_zval_pack_array_hash_inithash_done");
    emitter.instruction("str xzr, [sp, #56]");                                  // iter cursor = 0 starts a fresh walk
    emitter.instruction("str xzr, [sp, #48]");                                  // bucket_idx = 0 (reuses the init index slot)
    emitter.label("__rt_zval_pack_array_hash_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the elephc hash pointer
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the iter cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=next cursor, x1=key_lo, x2=key_hi, x3=val_lo, x4=val_hi, x5=val_tag
    emitter.instruction("str x0, [sp, #56]");                                   // save the next cursor for the following iteration
    emitter.instruction("cmp x0, #-1");                                         // is the walk done (-1)?
    emitter.instruction("b.eq __rt_zval_pack_array_hash_loop_done");            // stop once the hash walk is exhausted
    emitter.instruction("str x1, [sp, #64]");                                   // save key_lo (string ptr or int key)
    emitter.instruction("str x2, [sp, #72]");                                   // save key_hi (string length or -1)

    // -- pack the entry value into a child zval via the shared dispatch core --
    emitter.instruction("mov x0, x5");                                          // tag = value tag from the iterator
    emitter.instruction("mov x1, x3");                                          // lo = value low payload word
    emitter.instruction("mov x2, x4");                                          // hi = value high payload word
    emitter.instruction("bl __rt_zval_pack_element");                           // x0 = freshly built child zval pointer
    emitter.instruction("str x0, [sp, #80]");                                   // save the child zval pointer

    // -- dispatch on the key kind: key_hi == -1 marks an integer key --
    emitter.instruction("ldr x2, [sp, #72]");                                   // reload key_hi
    emitter.instruction("cmp x2, #-1");                                         // is this an integer key?
    emitter.instruction("b.eq __rt_zval_pack_array_hash_intkey");               // take the integer-key path

    // -- string key: h = DJBX33A(key), bucket.key = zend_string(key) --
    emitter.instruction("ldr x1, [sp, #64]");                                   // key_lo = string pointer
    emitter.instruction("ldr x2, [sp, #72]");                                   // key_hi = string length
    emitter.instruction("bl __rt_zval_djbx33a");                                // x0 = 64-bit DJBX33A hash with the high bit set
    emitter.instruction("str x0, [sp, #96]");                                   // save h (bucket.h)
    emitter.instruction("ldr x1, [sp, #64]");                                   // reload key_lo for zend_string_new
    emitter.instruction("ldr x2, [sp, #72]");                                   // reload key_hi for zend_string_new
    emitter.instruction("bl __rt_zval_string_new");                             // x0 = zend_string key pointer
    emitter.instruction("str x0, [sp, #104]");                                  // save the bucket key pointer
    emitter.instruction("b __rt_zval_pack_array_hash_place");                   // place the bucket with the string key

    // -- integer key: h = the int key, bucket.key = NULL --
    emitter.label("__rt_zval_pack_array_hash_intkey");
    emitter.instruction("ldr x0, [sp, #64]");                                   // h = the integer key payload
    emitter.instruction("str x0, [sp, #96]");                                   // save h (bucket.h)
    emitter.instruction("str xzr, [sp, #104]");                                 // bucket key = NULL (integer key has no zend_string)

    // -- place the bucket: compute nIndex, copy the child zval, set h/key, link the chain --
    emitter.label("__rt_zval_pack_array_hash_place");
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload h (64-bit)
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload nTableMask (64-bit sign-extended)
    emitter.instruction("orr x9, x9, x10");                                     // h | nTableMask (full 64-bit OR)
    emitter.instruction("sxtw x9, w9");                                         // nIndex = sign-extended low 32 bits of (h | nTableMask)
    emitter.instruction("str x9, [sp, #88]");                                   // save nIndex for the chain reads/writes

    // -- compute the bucket address: arData + bucket_idx * 32 --
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload arData
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload bucket_idx
    emitter.instruction("lsl x13, x12, #5");                                    // bucket_idx * 32 bytes per Bucket
    emitter.instruction("add x14, x11, x13");                                   // x14 = bucket address

    // -- copy the 16-byte child zval into the bucket value slot --
    emitter.instruction("ldr x15, [sp, #80]");                                  // reload the child zval pointer
    emitter.instruction("ldr x16, [x15]");                                      // load the child zval value
    emitter.instruction("str x16, [x14]");                                      // store the value into bucket.val@0
    emitter.instruction("ldr x16, [x15, #8]");                                  // load the child zval type_info + u2
    emitter.instruction("str x16, [x14, #8]");                                  // store type_info + u2 into bucket@8

    // -- set bucket.h@16 and bucket.key@24 --
    emitter.instruction("ldr x16, [sp, #96]");                                  // reload h
    emitter.instruction("str x16, [x14, #16]");                                 // bucket.h = h
    emitter.instruction("ldr x16, [sp, #104]");                                 // reload the key pointer
    emitter.instruction("str x16, [x14, #24]");                                 // bucket.key = zend_string or NULL

    // -- Z_NEXT chain: bucket.u2@12 = arHash[nIndex], then arHash[nIndex] = bucket_idx --
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload arData
    emitter.instruction("ldr x12, [sp, #88]");                                  // reload nIndex
    emitter.instruction("ldr w16, [x11, x12, lsl #2]");                         // w16 = arHash[nIndex] (the old chain head)
    emitter.instruction("str w16, [x14, #12]");                                 // bucket.u2@12 (Z_NEXT) = old chain head
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload bucket_idx
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload arData
    emitter.instruction("ldr x13, [sp, #88]");                                  // reload nIndex
    emitter.instruction("str w12, [x11, x13, lsl #2]");                         // arHash[nIndex] = bucket_idx (HT_IDX_TO_HASH on 64-bit)

    // -- release the temporary child zval block (ownership transfers to the bucket) --
    emitter.instruction("ldr x0, [sp, #80]");                                   // reload the child zval pointer
    emitter.instruction("bl __rt_heap_free");                                   // free the 16-byte child zval block only

    // -- advance to the next entry --
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload bucket_idx
    emitter.instruction("add x12, x12, #1");                                    // bucket_idx = bucket_idx + 1
    emitter.instruction("str x12, [sp, #48]");                                  // store the incremented bucket index
    emitter.instruction("b __rt_zval_pack_array_hash_loop");                    // continue the entry loop

    // -- done: allocate and fill the 56-byte HashTable structure --
    emitter.label("__rt_zval_pack_array_hash_loop_done");
    emitter.instruction("mov x0, #56");                                         // HashTable structures are 56 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = HashTable pointer
    emitter.instruction("str x0, [sp, #80]");                                   // save the HashTable pointer (reuse the zval slot)
    emitter.instruction("mov x9, #1");                                          // refcount starts at one owner
    emitter.instruction("str w9, [x0]");                                        // store refcount at offset 0 (32-bit)
    emitter.instruction("mov x9, #7");                                          // gc type_info = IS_ARRAY (7)
    emitter.instruction("str w9, [x0, #4]");                                    // store gc type_info at offset 4 (32-bit)
    emitter.instruction("mov x9, #0x40");                                       // HASH_FLAG_STATIC_KEYS (no packed flag)
    emitter.instruction("str w9, [x0, #8]");                                    // store flags at offset 8 (32-bit)
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload nTableMask (64-bit)
    emitter.instruction("str w9, [x0, #12]");                                   // store nTableMask at offset 12 (low 32 bits)
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload arData
    emitter.instruction("str x9, [x0, #16]");                                   // store arData at offset 16
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the count
    emitter.instruction("str w9, [x0, #24]");                                   // store nNumUsed at offset 24 (32-bit)
    emitter.instruction("str w9, [x0, #28]");                                   // store nNumOfElements at offset 28 (32-bit)
    emitter.instruction("mov x9, #-1");                                         // HT_INVALID_IDX for the internal pointer
    emitter.instruction("str w9, [x0, #32]");                                   // store nInternalPointer at offset 32 (32-bit)
    emitter.instruction("str xzr, [x0, #40]");                                  // nNextFreeElement = 0 at offset 40 (64-bit)
    emitter.instruction("str xzr, [x0, #48]");                                  // pDestructor = NULL at offset 48
    emitter.instruction("ldr x0, [sp, #80]");                                   // return the HashTable pointer
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the stack frame
    emitter.instruction("ret");                                                 // return the zend_array pointer in x0
}

/// x86_64 Linux implementation of `__rt_zval_pack_array_hash`.
/// Input:  rax = elephc assoc array (hash) pointer
/// Output: rax = zend_array (HashTable) pointer
fn emit_zval_pack_array_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_pack_array_hash ---");
    emitter.label_global("__rt_zval_pack_array_hash");

    // -- set up the frame and stash the elephc hash pointer --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 112");                                        // reserve hash/count/nTableSize/nTableMask/base/arData/bucket_idx/cursor/key_lo/key_hi/zval/nIndex/h/key slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the elephc hash pointer
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // count = hash header[0] (live entry count)
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the count (= nNumOfElements)

    // -- nTableSize = next power of two, minimum 8 --
    emitter.instruction("mov r9, 8");                                           // start at the minimum hash table size
    emitter.label("__rt_zval_pack_array_hash_pow2");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the count
    emitter.instruction("cmp r9, rcx");                                         // is the candidate size already >= count?
    emitter.instruction("jge __rt_zval_pack_array_hash_pow2_done");             // stop doubling once the table covers the count
    emitter.instruction("shl r9, 1");                                           // double the candidate size
    emitter.instruction("jmp __rt_zval_pack_array_hash_pow2");                  // retry the power-of-two search
    emitter.label("__rt_zval_pack_array_hash_pow2_done");
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save nTableSize

    // -- nTableMask = -nTableSize (sign-extended to 64-bit) --
    emitter.instruction("mov r10, r9");                                         // copy nTableSize
    emitter.instruction("neg r10");                                             // nTableMask = -nTableSize (unary two's-complement negate)
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save nTableMask

    // -- allocate the data block: nTableSize*4 hash index bytes + nTableSize*32 bucket bytes --
    emitter.instruction("mov rax, r9");                                         // copy nTableSize for the bucket byte count
    emitter.instruction("shl rax, 5");                                          // nTableSize * 32 bytes of bucket storage
    emitter.instruction("mov rcx, r9");                                         // copy nTableSize for the hash index byte count
    emitter.instruction("shl rcx, 2");                                          // nTableSize * 4 bytes of hash index storage
    emitter.instruction("add rax, rcx");                                        // total data block size = nTableSize * 36
    emitter.instruction("call __rt_heap_alloc");                                // rax = data block base (also the arHash base)
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the data block base
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload nTableSize
    emitter.instruction("shl r9, 2");                                           // hash index size = nTableSize * 4
    emitter.instruction("lea r9, [rax + r9]");                                  // arData = base + hash index size
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save arData

    // -- initialize every arHash slot to HT_INVALID_IDX (0xFFFFFFFF) --
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // arHash init loop index i = 0
    emitter.label("__rt_zval_pack_array_hash_inithash");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload nTableSize
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the init loop index
    emitter.instruction("cmp r10, r9");                                         // has every slot been initialized?
    emitter.instruction("jge __rt_zval_pack_array_hash_inithash_done");         // exit once every hash slot is initialized
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the data block base
    emitter.instruction("mov r12, r10");                                        // copy i for the byte offset
    emitter.instruction("shl r12, 2");                                          // i * 4 bytes per hash index slot
    emitter.instruction("lea r12, [r11 + r12]");                                // r12 = &arHash[i]
    emitter.instruction("mov DWORD PTR [r12], -1");                             // arHash[i] = HT_INVALID_IDX (0xFFFFFFFF)
    emitter.instruction("inc r10");                                             // advance to the next slot
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // store the incremented init index
    emitter.instruction("jmp __rt_zval_pack_array_hash_inithash");              // continue the hash-index init loop

    // -- entry loop: iterate the elephc hash in insertion order --
    emitter.label("__rt_zval_pack_array_hash_inithash_done");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // iter cursor = 0 starts a fresh walk
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // bucket_idx = 0 (reuses the init index slot)
    emitter.label("__rt_zval_pack_array_hash_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the elephc hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // reload the iter cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rax=next cursor, rdi=key_lo, rdx=key_hi, rcx=val_lo, r8=val_hi, r9=val_tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the next cursor for the following iteration
    emitter.instruction("cmp rax, -1");                                         // is the walk done (-1)?
    emitter.instruction("je __rt_zval_pack_array_hash_loop_done");              // stop once the hash walk is exhausted
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // save key_lo (string ptr or int key)
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save key_hi (string length or -1)

    // -- pack the entry value into a child zval via the shared dispatch core --
    emitter.instruction("mov rax, r9");                                         // tag = value tag from the iterator
    emitter.instruction("mov rdi, rcx");                                        // lo = value low payload word
    emitter.instruction("mov rsi, r8");                                         // hi = value high payload word
    emitter.instruction("call __rt_zval_pack_element");                         // rax = freshly built child zval pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save the child zval pointer

    // -- dispatch on the key kind: key_hi == -1 marks an integer key --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload key_hi
    emitter.instruction("cmp rdx, -1");                                         // is this an integer key?
    emitter.instruction("je __rt_zval_pack_array_hash_intkey");                 // take the integer-key path

    // -- string key: h = DJBX33A(key), bucket.key = zend_string(key) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // key_lo = string pointer (DJBX33A takes rdi)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // key_hi = string length (DJBX33A takes rsi)
    emitter.instruction("call __rt_zval_djbx33a");                              // rax = 64-bit DJBX33A hash with the high bit set
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save h (bucket.h)
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload key_lo for zend_string_new (takes rax)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload key_hi for zend_string_new (takes rdx)
    emitter.instruction("call __rt_zval_string_new");                           // rax = zend_string key pointer
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // save the bucket key pointer
    emitter.instruction("jmp __rt_zval_pack_array_hash_place");                 // place the bucket with the string key

    // -- integer key: h = the int key, bucket.key = NULL --
    emitter.label("__rt_zval_pack_array_hash_intkey");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // h = the integer key payload
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save h (bucket.h)
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // bucket key = NULL (integer key has no zend_string)

    // -- place the bucket: compute nIndex, copy the child zval, set h/key, link the chain --
    emitter.label("__rt_zval_pack_array_hash_place");
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // reload h (64-bit)
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload nTableMask (64-bit sign-extended)
    emitter.instruction("or r9, r10");                                          // h | nTableMask (full 64-bit OR)
    emitter.instruction("movsxd r9, r9d");                                      // nIndex = sign-extended low 32 bits of (h | nTableMask)
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // save nIndex for the chain reads/writes

    // -- compute the bucket address: arData + bucket_idx * 32 --
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload arData
    emitter.instruction("mov r12, QWORD PTR [rbp - 56]");                       // reload bucket_idx
    emitter.instruction("mov r13, r12");                                        // copy bucket_idx for the stride
    emitter.instruction("shl r13, 5");                                          // bucket_idx * 32 bytes per Bucket
    emitter.instruction("lea r14, [r11 + r13]");                                // r14 = bucket address

    // -- copy the 16-byte child zval into the bucket value slot --
    emitter.instruction("mov r15, QWORD PTR [rbp - 88]");                       // reload the child zval pointer
    emitter.instruction("mov rcx, QWORD PTR [r15]");                            // load the child zval value
    emitter.instruction("mov QWORD PTR [r14], rcx");                            // store the value into bucket.val@0
    emitter.instruction("mov rcx, QWORD PTR [r15 + 8]");                        // load the child zval type_info + u2
    emitter.instruction("mov QWORD PTR [r14 + 8], rcx");                        // store type_info + u2 into bucket@8

    // -- set bucket.h@16 and bucket.key@24 --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 104]");                      // reload h
    emitter.instruction("mov QWORD PTR [r14 + 16], rcx");                       // bucket.h = h
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                      // reload the key pointer
    emitter.instruction("mov QWORD PTR [r14 + 24], rcx");                       // bucket.key = zend_string or NULL

    // -- Z_NEXT chain: bucket.u2@12 = arHash[nIndex], then arHash[nIndex] = bucket_idx --
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload arData
    emitter.instruction("mov r12, QWORD PTR [rbp - 96]");                       // reload nIndex
    emitter.instruction("mov r13, r12");                                        // copy nIndex for the stride
    emitter.instruction("shl r13, 2");                                          // nIndex * 4 bytes per hash index slot
    emitter.instruction("mov ecx, DWORD PTR [r11 + r13]");                      // ecx = arHash[nIndex] (the old chain head, 32-bit)
    emitter.instruction("mov DWORD PTR [r14 + 12], ecx");                       // bucket.u2@12 (Z_NEXT) = old chain head
    emitter.instruction("mov r12, QWORD PTR [rbp - 56]");                       // reload bucket_idx
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload arData
    emitter.instruction("mov r13, QWORD PTR [rbp - 96]");                       // reload nIndex
    emitter.instruction("mov r14, r13");                                        // copy nIndex for the stride
    emitter.instruction("shl r14, 2");                                          // nIndex * 4 bytes per hash index slot
    emitter.instruction("mov DWORD PTR [r11 + r14], r12d");                     // arHash[nIndex] = bucket_idx (HT_IDX_TO_HASH on 64-bit)

    // -- release the temporary child zval block (ownership transfers to the bucket) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the child zval pointer
    emitter.instruction("call __rt_heap_free");                                 // free the 16-byte child zval block only

    // -- advance to the next entry --
    emitter.instruction("mov r12, QWORD PTR [rbp - 56]");                       // reload bucket_idx
    emitter.instruction("inc r12");                                             // bucket_idx = bucket_idx + 1
    emitter.instruction("mov QWORD PTR [rbp - 56], r12");                       // store the incremented bucket index
    emitter.instruction("jmp __rt_zval_pack_array_hash_loop");                  // continue the entry loop

    // -- done: allocate and fill the 56-byte HashTable structure --
    emitter.label("__rt_zval_pack_array_hash_loop_done");
    emitter.instruction("mov rax, 56");                                         // HashTable structures are 56 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = HashTable pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save the HashTable pointer (reuse the zval slot)
    emitter.instruction("mov DWORD PTR [rax], 1");                              // refcount = 1 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 4], 7");                          // gc type_info = IS_ARRAY (7)
    emitter.instruction("mov DWORD PTR [rax + 8], 64");                         // HASH_FLAG_STATIC_KEYS = 0x40 (no packed flag)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload nTableMask (64-bit)
    emitter.instruction("mov DWORD PTR [rax + 12], ecx");                       // store nTableMask at offset 12 (low 32 bits)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload arData
    emitter.instruction("mov QWORD PTR [rax + 16], rcx");                       // store arData at offset 16
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the count
    emitter.instruction("mov DWORD PTR [rax + 24], ecx");                       // store nNumUsed at offset 24 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 28], ecx");                       // store nNumOfElements at offset 28 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 32], -1");                        // nInternalPointer = HT_INVALID_IDX (32-bit)
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // nNextFreeElement = 0 at offset 40 (64-bit)
    emitter.instruction("mov QWORD PTR [rax + 48], 0");                         // pDestructor = NULL
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // return the HashTable pointer
    emitter.instruction("add rsp, 112");                                        // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the zend_array pointer in rax
}
