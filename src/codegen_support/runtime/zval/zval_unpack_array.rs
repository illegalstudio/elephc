//! Purpose:
//! Emits the `__rt_zval_unpack_array` runtime helper that rebuilds an elephc
//! array from a PHP `zend_array` (HashTable).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and directly from `__rt_zval_unpack` for the `IS_ARRAY` kind.
//!
//! Key details:
//! - Input: `x0` / `rax` = zend_array (HashTable) pointer.
//! - Output: `x0` / `rax` = runtime array tag (4 = indexed, 5 = associative),
//!   `x1` / `rdx` = elephc array pointer (freshly allocated, refcount 1).
//! - Packed HashTables (`nTableMask == -2`, the PHP `HT_MIN_MASK`) are rebuilt as
//!   an elephc `Array<Mixed>` whose elements are boxed cells produced by
//!   `__rt_zval_unpack` per 32-byte Bucket.
//! - Hash (associative) HashTables are rebuilt as an elephc assoc array via
//!   `__rt_hash_new` + `__rt_zval_unpack` per Bucket, with string keys read from
//!   the bucket's `zend_string` and integer keys taken from `bucket.h`.
//! - Bucket layout: `val` (16 bytes) @0, `h` (8) @16, `key` (8) @24; for packed
//!   arrays `h` is the integer key and `key` is NULL, so the bucket value slot
//!   has the same layout as a standalone 16-byte zval.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_unpack_array: rebuild an elephc array from a zend_array.
/// Input:  x0 / rax = zend_array pointer
/// Output: x0 / rax = runtime array tag, x1 / rdx = elephc array pointer
pub fn emit_zval_unpack_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_unpack_array_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_unpack_array ---");
    emitter.label_global("__rt_zval_unpack_array");

    // -- set up a shared 96-byte frame and save the HashTable pointer --
    emitter.instruction("sub sp, sp, #96");                                     // reserve HT/array/i/nNumUsed/bucket/key_lo/key_hi/cell slots plus frame records
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the zend_array pointer across helper calls

    // -- dispatch on nTableMask: -2 (HT_MIN_MASK) selects the packed layout --
    emitter.instruction("ldr w9, [x0, #12]");                                   // load nTableMask from the HashTable header
    emitter.instruction("mov w10, #-2");                                        // HT_MIN_MASK marks a PHP packed HashTable
    emitter.instruction("cmp w9, w10");                                         // is this a packed array?
    emitter.instruction("b.ne __rt_zval_unpack_array_hash");                    // otherwise rebuild as an associative hash

    // -- read the packed view: arData and nNumUsed --
    emitter.instruction("ldr x11, [x0, #16]");                                  // arData = first bucket base
    emitter.instruction("ldr w12, [x0, #24]");                                  // nNumUsed = number of occupied buckets (zero-extended)
    emitter.instruction("str x12, [sp, #24]");                                  // save nNumUsed across the array allocation

    // -- allocate an elephc Array<Mixed> sized for nNumUsed 8-byte cell slots --
    emitter.instruction("mov x0, x12");                                         // capacity = nNumUsed
    emitter.instruction("mov x1, #8");                                          // elem_size = 8 (each slot holds a Mixed cell pointer)
    emitter.instruction("bl __rt_array_new");                                   // x0 = fresh indexed array (refcount 1, value_type 0)
    emitter.instruction("str x0, [sp, #8]");                                    // save the rebuilt array pointer

    // -- fix the kind word: value_type 7 marks Mixed-cell element storage --
    emitter.instruction("mov x9, #0x8702");                                     // kind = indexed(2) | value_type 7 | COW(0x8000)
    emitter.instruction("str x9, [x0, #-8]");                                   // install the Mixed-element kind word in the heap header

    // -- set the length = nNumUsed so the array reports its occupied size --
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload nNumUsed
    emitter.instruction("str x12, [x0]");                                       // header[0]: length = nNumUsed

    // -- initialize the loop index i = 0 --
    emitter.instruction("str xzr, [sp, #16]");                                  // loop index i starts at the first bucket

    // -- element loop: convert each bucket value into a Mixed cell and store it --
    emitter.label("__rt_zval_unpack_array_packed_loop");
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload the loop index
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload nNumUsed
    emitter.instruction("cmp x14, x12");                                        // has every occupied bucket been processed?
    emitter.instruction("b.ge __rt_zval_unpack_array_packed_done");             // exit the loop once i reaches nNumUsed

    // -- compute the bucket address: arData + i * 32 --
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the HashTable (arData is clobbered by the helper)
    emitter.instruction("ldr x11, [x11, #16]");                                 // arData = first bucket base
    emitter.instruction("lsl x15, x14, #5");                                    // i * 32 bytes per Bucket
    emitter.instruction("add x0, x11, x15");                                    // x0 = bucket address (bucket val slot is zval-shaped)
    emitter.instruction("bl __rt_zval_unpack");                                 // x0 = boxed Mixed cell for this bucket value

    // -- store the cell at arr + 24 + i * 8 --
    emitter.instruction("ldr x13, [sp, #8]");                                   // reload the rebuilt array pointer
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload i (clobbered by the unpack call) before the slot stride multiply
    emitter.instruction("lsl x15, x14, #3");                                    // i * 8 bytes per Mixed-cell slot
    emitter.instruction("add x16, x13, #24");                                   // elements start after the 24-byte array header
    emitter.instruction("add x16, x16, x15");                                   // x16 = destination element slot
    emitter.instruction("str x0, [x16]");                                       // arr[i] = owned Mixed cell pointer

    // -- advance the loop index --
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload the loop index
    emitter.instruction("add x14, x14, #1");                                    // i = i + 1
    emitter.instruction("str x14, [sp, #16]");                                  // store the next loop index
    emitter.instruction("b __rt_zval_unpack_array_packed_loop");                // continue with the next bucket

    // -- packed done: return tag 4 (indexed array) and the rebuilt array --
    emitter.label("__rt_zval_unpack_array_packed_done");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the rebuilt array pointer
    emitter.instruction("mov x0, #4");                                          // tag = 4 (indexed array)
    emitter.instruction("b __rt_zval_unpack_array_epilogue");                   // join the shared return path

    // -- hash path: rebuild an elephc assoc array from the zend_array --
    emitter.label("__rt_zval_unpack_array_hash");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the HashTable pointer
    emitter.instruction("ldr w12, [x0, #24]");                                  // nNumUsed = number of occupied buckets (zero-extended)
    emitter.instruction("str x12, [sp, #24]");                                  // save nNumUsed

    // -- create an elephc hash with room for nNumUsed mixed entries (no growth needed) --
    emitter.instruction("add x0, x12, #8");                                     // capacity = nNumUsed + 8 (headroom avoids mid-insert growth)
    emitter.instruction("mov x1, #7");                                          // value_type = 7 (mixed entries)
    emitter.instruction("bl __rt_hash_new");                                    // x0 = fresh hash (refcount 1)
    emitter.instruction("str x0, [sp, #8]");                                    // save the hash pointer (updated by __rt_hash_set)

    // -- initialize the loop index i = 0 --
    emitter.instruction("str xzr, [sp, #16]");                                  // loop index i starts at the first bucket

    // -- element loop: unpack each bucket value and insert it under its key --
    emitter.label("__rt_zval_unpack_array_hash_loop");
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload the loop index
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload nNumUsed
    emitter.instruction("cmp x14, x12");                                        // has every occupied bucket been processed?
    emitter.instruction("b.ge __rt_zval_unpack_array_hash_done");               // exit the loop once i reaches nNumUsed

    // -- compute the bucket address and unpack its value into a Mixed cell --
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the HashTable pointer
    emitter.instruction("ldr x11, [x11, #16]");                                 // arData = first bucket base
    emitter.instruction("lsl x15, x14, #5");                                    // i * 32 bytes per Bucket
    emitter.instruction("add x11, x11, x15");                                   // x11 = bucket address
    emitter.instruction("str x11, [sp, #32]");                                  // save the bucket address across the unpack call
    emitter.instruction("mov x0, x11");                                         // x0 = bucket address (bucket val slot is zval-shaped)
    emitter.instruction("bl __rt_zval_unpack");                                 // x0 = boxed Mixed cell for this bucket value
    emitter.instruction("str x0, [sp, #56]");                                   // save the owned value cell across the hash insert

    // -- read the bucket key and classify it (NULL = integer key, else zend_string) --
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the bucket address
    emitter.instruction("ldr x12, [x11, #24]");                                 // load bucket.key (NULL for integer keys)
    emitter.instruction("cbz x12, __rt_zval_unpack_array_hash_intkey");         // NULL key -> integer key path

    // -- string key: bytes at zend_string+24, length at zend_string+16 --
    emitter.instruction("ldr x13, [x12, #16]");                                 // load the zend_string length
    emitter.instruction("add x12, x12, #24");                                   // x12 = zend_string val[] base (key bytes)
    emitter.instruction("str x12, [sp, #40]");                                  // save key_lo = key byte pointer
    emitter.instruction("str x13, [sp, #48]");                                  // save key_hi = key byte length
    emitter.instruction("b __rt_zval_unpack_array_hash_set");                   // proceed to the insert

    // -- integer key: h = bucket.h (the integer key payload) --
    emitter.label("__rt_zval_unpack_array_hash_intkey");
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the bucket address
    emitter.instruction("ldr x12, [x11, #16]");                                 // load bucket.h (the integer key)
    emitter.instruction("str x12, [sp, #40]");                                  // save key_lo = the integer key
    emitter.instruction("mov x13, #-1");                                        // integer-key sentinel
    emitter.instruction("str x13, [sp, #48]");                                  // save key_hi = -1 (marks an integer key)

    // -- insert the entry: __rt_hash_set(tbl, key_lo, key_hi, val_lo, val_hi, val_tag) --
    emitter.label("__rt_zval_unpack_array_hash_set");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // key_lo (string pointer or integer key)
    emitter.instruction("ldr x2, [sp, #48]");                                   // key_hi (string length or -1)
    emitter.instruction("ldr x3, [sp, #56]");                                   // val_lo = the owned Mixed cell
    emitter.instruction("mov x4, xzr");                                         // val_hi = 0
    emitter.instruction("mov x5, #7");                                          // val_tag = 7 (mixed)
    emitter.instruction("bl __rt_hash_set");                                    // x0 = hash pointer (may reallocate)
    emitter.instruction("str x0, [sp, #8]");                                    // update the hash pointer after a possible reallocation

    // -- advance the loop index --
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload the loop index
    emitter.instruction("add x14, x14, #1");                                    // i = i + 1
    emitter.instruction("str x14, [sp, #16]");                                  // store the next loop index
    emitter.instruction("b __rt_zval_unpack_array_hash_loop");                  // continue with the next bucket

    // -- hash done: return tag 5 (associative array) and the rebuilt hash --
    emitter.label("__rt_zval_unpack_array_hash_done");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the rebuilt hash pointer
    emitter.instruction("mov x0, #5");                                          // tag = 5 (associative array)
    emitter.instruction("b __rt_zval_unpack_array_epilogue");                   // join the shared return path

    // -- shared epilogue: x0 = tag, x1 = array/hash pointer --
    emitter.label("__rt_zval_unpack_array_epilogue");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return tag in x0, array pointer in x1
}

/// x86_64 Linux implementation of `__rt_zval_unpack_array`.
/// Rebuilds a packed zend_array into an elephc `Array<Mixed>` and a hash
/// zend_array into an elephc assoc array.
/// Input:  rax = zend_array pointer
/// Output: rax = runtime array tag, rdx = elephc array pointer
fn emit_zval_unpack_array_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_unpack_array ---");
    emitter.label_global("__rt_zval_unpack_array");

    // -- set up a shared frame and save the HashTable pointer --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 80");                                         // reserve HT/array/i/nNumUsed/bucket/key_lo/key_hi/cell slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the zend_array pointer across helper calls

    // -- dispatch on nTableMask: -2 (HT_MIN_MASK) selects the packed layout --
    emitter.instruction("mov ecx, DWORD PTR [rax + 12]");                       // load nTableMask from the HashTable header
    emitter.instruction("cmp ecx, -2");                                         // is this a packed HashTable (HT_MIN_MASK)?
    emitter.instruction("jne __rt_zval_unpack_array_hash");                     // otherwise rebuild as an associative hash

    // -- read the packed view: arData and nNumUsed --
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // arData = first bucket base
    emitter.instruction("mov r12d, DWORD PTR [rax + 24]");                      // nNumUsed = occupied bucket count (zero-extended)
    emitter.instruction("mov QWORD PTR [rbp - 32], r12");                       // save nNumUsed across the array allocation

    // -- allocate an elephc Array<Mixed> sized for nNumUsed 8-byte cell slots --
    emitter.instruction("mov rdi, r12");                                        // capacity = nNumUsed
    emitter.instruction("mov rsi, 8");                                          // elem_size = 8 (each slot holds a Mixed cell pointer)
    emitter.instruction("call __rt_array_new");                                 // rax = fresh indexed array (refcount 1, value_type 0)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the rebuilt array pointer

    // -- fix the kind word: value_type 7 marks Mixed-cell element storage --
    // The full word overwrite must preserve the x86_64 heap ownership marker
    // (`0x454C5048` in the high 32 bits) that `__rt_array_new` stamped; without
    // it `__rt_heap_kind` reports kind 0 and foreach/heap_free treat the rebuilt
    // array as a non-heap value (arm64 has no such marker).
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(0x8702))); // x86_64 heap marker | indexed(2) | value_type 7 | COW(0x8000)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the Mixed-element kind word in the heap header

    // -- set the length = nNumUsed so the array reports its occupied size --
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // reload nNumUsed
    emitter.instruction("mov QWORD PTR [rax], r12");                            // header[0]: length = nNumUsed

    // -- initialize the loop index i = 0 --
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // loop index i starts at the first bucket

    // -- element loop: convert each bucket value into a Mixed cell and store it --
    emitter.label("__rt_zval_unpack_array_packed_loop");
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // reload nNumUsed
    emitter.instruction("cmp r14, r12");                                        // has every occupied bucket been processed?
    emitter.instruction("jge __rt_zval_unpack_array_packed_done");              // exit the loop once i reaches nNumUsed

    // -- compute the bucket address: arData + i * 32 --
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the HashTable (arData is clobbered by the helper)
    emitter.instruction("mov r11, QWORD PTR [r11 + 16]");                       // arData = first bucket base
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index before the stride multiply
    emitter.instruction("shl r14, 5");                                          // i * 32 bytes per Bucket
    emitter.instruction("lea rax, [r11 + r14]");                                // rax = bucket address (bucket val slot is zval-shaped)
    emitter.instruction("call __rt_zval_unpack");                               // rax = boxed Mixed cell for this bucket value

    // -- store the cell at arr + 24 + i * 8 --
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // reload the rebuilt array pointer
    emitter.instruction("add r13, 24");                                         // elements start after the 24-byte array header
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index before the slot stride multiply
    emitter.instruction("shl r14, 3");                                          // i * 8 bytes per Mixed-cell slot
    emitter.instruction("add r13, r14");                                        // r13 = destination element slot
    emitter.instruction("mov QWORD PTR [r13], rax");                            // arr[i] = owned Mixed cell pointer

    // -- advance the loop index --
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index
    emitter.instruction("inc r14");                                             // i = i + 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r14");                       // store the next loop index
    emitter.instruction("jmp __rt_zval_unpack_array_packed_loop");              // continue with the next bucket

    // -- packed done: return tag 4 (indexed array) and the rebuilt array --
    emitter.label("__rt_zval_unpack_array_packed_done");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the rebuilt array pointer into the second return register
    emitter.instruction("mov eax, 4");                                          // tag = 4 (indexed array)
    emitter.instruction("jmp __rt_zval_unpack_array_epilogue");                 // join the shared return path

    // -- hash path: rebuild an elephc assoc array from the zend_array --
    emitter.label("__rt_zval_unpack_array_hash");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the HashTable pointer
    emitter.instruction("mov r12d, DWORD PTR [rax + 24]");                      // nNumUsed = occupied bucket count (zero-extended)
    emitter.instruction("mov QWORD PTR [rbp - 32], r12");                       // save nNumUsed

    // -- create an elephc hash with room for nNumUsed mixed entries (no growth needed) --
    emitter.instruction("lea rdi, [r12 + 8]");                                  // capacity = nNumUsed + 8 (headroom avoids mid-insert growth)
    emitter.instruction("mov rsi, 7");                                          // value_type = 7 (mixed entries)
    emitter.instruction("call __rt_hash_new");                                  // rax = fresh hash (refcount 1)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the hash pointer (updated by __rt_hash_set)

    // -- initialize the loop index i = 0 --
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // loop index i starts at the first bucket

    // -- element loop: unpack each bucket value and insert it under its key --
    emitter.label("__rt_zval_unpack_array_hash_loop");
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // reload nNumUsed
    emitter.instruction("cmp r14, r12");                                        // has every occupied bucket been processed?
    emitter.instruction("jge __rt_zval_unpack_array_hash_done");                // exit the loop once i reaches nNumUsed

    // -- compute the bucket address and unpack its value into a Mixed cell --
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the HashTable pointer
    emitter.instruction("mov r11, QWORD PTR [r11 + 16]");                       // arData = first bucket base
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index before the stride multiply
    emitter.instruction("shl r14, 5");                                          // i * 32 bytes per Bucket
    emitter.instruction("lea r11, [r11 + r14]");                                // r11 = bucket address
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the bucket address across the unpack call
    emitter.instruction("mov rax, r11");                                        // rax = bucket address (bucket val slot is zval-shaped)
    emitter.instruction("call __rt_zval_unpack");                               // rax = boxed Mixed cell for this bucket value
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the owned value cell across the hash insert

    // -- read the bucket key and classify it (NULL = integer key, else zend_string) --
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the bucket address
    emitter.instruction("mov r12, QWORD PTR [r11 + 24]");                       // load bucket.key (NULL for integer keys)
    emitter.instruction("test r12, r12");                                       // is the key NULL?
    emitter.instruction("jz __rt_zval_unpack_array_hash_intkey");               // NULL key -> integer key path

    // -- string key: bytes at zend_string+24, length at zend_string+16 --
    emitter.instruction("mov r13, QWORD PTR [r12 + 16]");                       // load the zend_string length
    emitter.instruction("lea r12, [r12 + 24]");                                 // r12 = zend_string val[] base (key bytes)
    emitter.instruction("mov QWORD PTR [rbp - 48], r12");                       // save key_lo = key byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r13");                       // save key_hi = key byte length
    emitter.instruction("jmp __rt_zval_unpack_array_hash_set");                 // proceed to the insert

    // -- integer key: h = bucket.h (the integer key payload) --
    emitter.label("__rt_zval_unpack_array_hash_intkey");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the bucket address
    emitter.instruction("mov r12, QWORD PTR [r11 + 16]");                       // load bucket.h (the integer key)
    emitter.instruction("mov QWORD PTR [rbp - 48], r12");                       // save key_lo = the integer key
    emitter.instruction("mov QWORD PTR [rbp - 56], -1");                        // save key_hi = -1 (marks an integer key)

    // -- insert the entry: __rt_hash_set(tbl, key_lo, key_hi, val_lo, val_hi, val_tag) --
    emitter.label("__rt_zval_unpack_array_hash_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // key_lo (string pointer or integer key)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // key_hi (string length or -1)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // val_lo = the owned Mixed cell
    emitter.instruction("xor r8d, r8d");                                        // val_hi = 0
    emitter.instruction("mov r9d, 7");                                          // val_tag = 7 (mixed)
    emitter.instruction("call __rt_hash_set");                                  // rax = hash pointer (may reallocate)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // update the hash pointer after a possible reallocation

    // -- advance the loop index --
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // reload the loop index
    emitter.instruction("inc r14");                                             // i = i + 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r14");                       // store the next loop index
    emitter.instruction("jmp __rt_zval_unpack_array_hash_loop");                // continue with the next bucket

    // -- hash done: return tag 5 (associative array) and the rebuilt hash --
    emitter.label("__rt_zval_unpack_array_hash_done");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the rebuilt hash pointer into the second return register
    emitter.instruction("mov eax, 5");                                          // tag = 5 (associative array)
    emitter.instruction("jmp __rt_zval_unpack_array_epilogue");                 // join the shared return path

    // -- shared epilogue: rax = tag, rdx = array/hash pointer --
    emitter.label("__rt_zval_unpack_array_epilogue");
    emitter.instruction("add rsp, 80");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return tag in rax, array pointer in rdx
}
