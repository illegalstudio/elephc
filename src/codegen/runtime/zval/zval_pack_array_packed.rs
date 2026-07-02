//! Purpose:
//! Emits the `__rt_zval_pack_array_packed` runtime helper that builds a PHP
//! packed `zend_array` (HashTable) from an elephc indexed array.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::zval`,
//!   and from `__rt_zval_pack_element` when packing nested indexed arrays.
//!
//! Key details:
//! - Builds a packed HashTable (`nTableMask = -2`, two `HT_INVALID_IDX`
//!   sentinels, then `nTableSize` 32-byte Buckets). Each elephc element is
//!   converted via `__rt_zval_pack_element` and copied into the bucket value
//!   slot; the temporary zval block is then released so the bucket owns the
//!   freshly allocated string/array children.
//! - elephc indexed-array layout: `length@0`, `capacity@8`, `elem_size@16`,
//!   elements at `+24`; the element `value_type` is byte 1 of the kind word at
//!   `arr - 8` (0=int, 1=string, 2=float, 3=bool, 7=mixed cell ptr, 11=tagged).
//! - Emits both ARM64 and x86_64 variants gated on `emitter.target.arch`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// zval_pack_array_packed: build a PHP packed HashTable from an elephc indexed array.
/// Input:  x0 / rax = elephc indexed array pointer
/// Output: x0 / rax = zend_array (HashTable) pointer
pub fn emit_zval_pack_array_packed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_pack_array_packed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_pack_array_packed ---");
    emitter.label_global("__rt_zval_pack_array_packed");

    // -- set up stack frame and read the elephc indexed array header --
    emitter.instruction("sub sp, sp, #96");                                     // reserve header/loop slots plus frame records
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the elephc array pointer
    emitter.instruction("ldr x1, [x0]");                                        // load the element count
    emitter.instruction("str x1, [sp, #8]");                                    // save the length
    emitter.instruction("ldr x1, [x0, #16]");                                   // load the element size
    emitter.instruction("str x1, [sp, #16]");                                   // save the elem_size
    emitter.instruction("ldr x1, [x0, #-8]");                                   // load the kind word (value_type is byte 1)
    emitter.instruction("lsr x1, x1, #8");                                      // shift the value_type byte into the low position
    emitter.instruction("and x1, x1, #0x7f");                                   // isolate the 7-bit value_type, excluding the COW flag in bit 15
    emitter.instruction("str x1, [sp, #24]");                                   // save the value_type
    emitter.instruction("add x1, x0, #24");                                     // elements start right after the 24-byte header
    emitter.instruction("str x1, [sp, #56]");                                   // save the elements base

    // -- nTableSize = next power of two, minimum 8 --
    emitter.instruction("mov x9, #8");                                          // start at the minimum packed table size
    emitter.label("__rt_zval_pack_array_packed_pow2");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the length
    emitter.instruction("cmp x9, x1");                                          // is the candidate size already >= length?
    emitter.instruction("b.ge __rt_zval_pack_array_packed_pow2_done");          // stop doubling once the table covers the length
    emitter.instruction("lsl x9, x9, #1");                                      // double the candidate size
    emitter.instruction("b __rt_zval_pack_array_packed_pow2");                  // retry the power-of-two search
    emitter.label("__rt_zval_pack_array_packed_pow2_done");
    emitter.instruction("str x9, [sp, #40]");                                   // save nTableSize

    // -- allocate the data block (8 sentinel bytes + nTableSize * 32-byte buckets) --
    emitter.instruction("lsl x0, x9, #5");                                      // nTableSize * 32 bytes of bucket storage
    emitter.instruction("add x0, x0, #8");                                      // add the two 4-byte sentinel slots
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = data block base
    emitter.instruction("str x0, [sp, #32]");                                   // save the data block pointer
    emitter.instruction("mov x9, #-1");                                         // HT_INVALID_IDX = 0xFFFFFFFF
    emitter.instruction("str w9, [x0]");                                        // stamp the first packed sentinel
    emitter.instruction("str w9, [x0, #4]");                                    // stamp the second packed sentinel
    emitter.instruction("add x9, x0, #8");                                      // arData = data block base + 8
    emitter.instruction("str x9, [sp, #64]");                                   // save arData
    emitter.instruction("str xzr, [sp, #48]");                                  // initialize the loop index i = 0

    // -- element loop: convert each elephc element into a bucket zval --
    emitter.label("__rt_zval_pack_array_packed_loop");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the length
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the loop index
    emitter.instruction("cmp x0, x1");                                          // has every element been processed?
    emitter.instruction("b.ge __rt_zval_pack_array_packed_loop_done");          // exit the loop once every element is packed

    // -- compute the element address: elements_base + i * elem_size --
    emitter.instruction("ldr x14, [sp, #56]");                                  // load the elements base
    emitter.instruction("ldr x15, [sp, #16]");                                  // load the element size
    emitter.instruction("ldr x16, [sp, #48]");                                  // reload the loop index
    emitter.instruction("mul x17, x16, x15");                                   // i * elem_size
    emitter.instruction("add x14, x14, x17");                                   // x14 = element address

    // -- dispatch on the value_type to stage (tag, lo, hi) for pack_element --
    emitter.instruction("ldr x15, [sp, #24]");                                  // reload the value_type
    emitter.instruction("cmp x15, #0");                                         // value_type 0 = int
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_int");             // stage the int element for pack_element
    emitter.instruction("cmp x15, #1");                                         // value_type 1 = string
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_str");             // stage the string element for pack_element
    emitter.instruction("cmp x15, #2");                                         // value_type 2 = float
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_float");           // stage the float element for pack_element
    emitter.instruction("cmp x15, #3");                                         // value_type 3 = bool
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_bool");            // stage the bool element for pack_element
    emitter.instruction("cmp x15, #7");                                         // value_type 7 = nested mixed cell
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_mixed");           // stage the nested Mixed cell for pack_element
    emitter.instruction("cmp x15, #4");                                         // value_type 4 = nested indexed array
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_idxarr");          // stage the nested indexed array for pack_element
    emitter.instruction("cmp x15, #5");                                         // value_type 5 = nested associative array
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_hasharr");         // stage the nested associative array for pack_element
    emitter.instruction("cmp x15, #11");                                        // value_type 11 = tagged scalar
    emitter.instruction("b.eq __rt_zval_pack_array_packed_vt_tagged");          // stage the tagged scalar for pack_element
    emitter.instruction("b __rt_zval_pack_array_packed_vt_null");               // unknown kinds pack as null

    emitter.label("__rt_zval_pack_array_packed_vt_int");
    emitter.instruction("ldr x1, [x14]");                                       // lo = integer value
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("mov x0, #0");                                          // tag = 0 (int)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_float");
    emitter.instruction("ldr x1, [x14]");                                       // lo = float bits
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("mov x0, #2");                                          // tag = 2 (float)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_bool");
    emitter.instruction("ldr x1, [x14]");                                       // lo = bool payload
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("mov x0, #3");                                          // tag = 3 (bool)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_mixed");
    emitter.instruction("ldr x1, [x14]");                                       // lo = nested mixed cell pointer
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("mov x0, #7");                                          // tag = 7 (nested)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_idxarr");
    emitter.instruction("ldr x1, [x14]");                                       // lo = nested indexed-array pointer
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("mov x0, #4");                                          // tag = 4 (indexed array)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_hasharr");
    emitter.instruction("ldr x1, [x14]");                                       // lo = nested associative-array pointer
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("mov x0, #5");                                          // tag = 5 (associative array)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_tagged");
    emitter.instruction("ldr x1, [x14]");                                       // lo = tagged payload
    emitter.instruction("ldr x0, [x14, #8]");                                   // tag = tagged runtime tag
    emitter.instruction("mov x2, xzr");                                         // hi = 0
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_str");
    emitter.instruction("ldr x1, [x14]");                                       // lo = string pointer
    emitter.instruction("ldr x2, [x14, #8]");                                   // hi = string length
    emitter.instruction("mov x0, #1");                                          // tag = 1 (string)
    emitter.instruction("b __rt_zval_pack_array_packed_call_elem");             // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, xzr");                                         // lo = 0
    emitter.instruction("mov x2, xzr");                                         // hi = 0

    emitter.label("__rt_zval_pack_array_packed_call_elem");
    emitter.instruction("bl __rt_zval_pack_element");                           // x0 = freshly built child zval pointer

    // -- copy the 16-byte zval into the bucket at arData + i*32 --
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload arData
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload i
    emitter.instruction("lsl x11, x10, #5");                                    // i * 32 bytes per bucket
    emitter.instruction("add x12, x9, x11");                                    // x12 = bucket address
    emitter.instruction("ldr x13, [x0]");                                       // load the child zval value
    emitter.instruction("str x13, [x12]");                                      // store the value into the bucket
    emitter.instruction("ldr x13, [x0, #8]");                                   // load the child zval type_info + u2
    emitter.instruction("str x13, [x12, #8]");                                  // store type_info + u2 into the bucket
    emitter.instruction("ldr x14, [sp, #48]");                                  // reload i for the integer key
    emitter.instruction("str x14, [x12, #16]");                                 // bucket.h = i (packed integer key)
    emitter.instruction("str xzr, [x12, #24]");                                 // bucket.key = NULL (packed has no string key)

    // -- release the temporary child zval block (ownership transfers to the bucket) --
    emitter.instruction("bl __rt_heap_free");                                   // free the 16-byte child zval block only
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload i
    emitter.instruction("add x9, x9, #1");                                      // advance to the next element
    emitter.instruction("str x9, [sp, #48]");                                   // store the incremented index
    emitter.instruction("b __rt_zval_pack_array_packed_loop");                  // continue the element loop

    emitter.label("__rt_zval_pack_array_packed_loop_done");

    // -- allocate and fill the 56-byte HashTable structure --
    emitter.instruction("mov x0, #56");                                         // HashTable structures are 56 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = HashTable pointer
    emitter.instruction("str x0, [sp, #72]");                                   // save the HashTable pointer
    emitter.instruction("mov x9, #1");                                          // refcount starts at one owner
    emitter.instruction("str w9, [x0]");                                        // store refcount at offset 0 (32-bit)
    emitter.instruction("mov x9, #7");                                          // gc type_info = IS_ARRAY (7)
    emitter.instruction("str w9, [x0, #4]");                                    // store gc type_info at offset 4 (32-bit)
    emitter.instruction("mov x9, #0x60");                                       // HASH_FLAG_PACKED | HASH_FLAG_STATIC_KEYS
    emitter.instruction("str w9, [x0, #8]");                                    // store flags at offset 8 (32-bit)
    emitter.instruction("mov x9, #-2");                                         // HT_MIN_MASK = -2 marks a packed table
    emitter.instruction("str w9, [x0, #12]");                                   // store nTableMask at offset 12 (32-bit)
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the data block base
    emitter.instruction("add x9, x9, #8");                                      // arData = data block base + 8
    emitter.instruction("str x9, [x0, #16]");                                   // store arData at offset 16
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the length
    emitter.instruction("str w9, [x0, #24]");                                   // store nNumUsed at offset 24 (32-bit)
    emitter.instruction("str w9, [x0, #28]");                                   // store nNumOfElements at offset 28 (32-bit)
    emitter.instruction("mov x9, #-1");                                         // HT_INVALID_IDX for the internal pointer
    emitter.instruction("str w9, [x0, #32]");                                   // store nInternalPointer at offset 32 (32-bit)
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the length for nNextFreeElement
    emitter.instruction("str x9, [x0, #40]");                                   // store nNextFreeElement at offset 40 (64-bit)
    emitter.instruction("str xzr, [x0, #48]");                                  // store pDestructor = NULL at offset 48
    emitter.instruction("ldr x0, [sp, #72]");                                   // return the HashTable pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return the zend_array pointer in x0
}

/// x86_64 Linux implementation of `__rt_zval_pack_array_packed`.
/// Input:  rax = elephc indexed array pointer
/// Output: rax = zend_array (HashTable) pointer
fn emit_zval_pack_array_packed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_pack_array_packed ---");
    emitter.label_global("__rt_zval_pack_array_packed");

    // -- set up stack frame and read the elephc indexed array header --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // reserve header/loop slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the elephc array pointer
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the element count
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the length
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // load the element size
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the elem_size
    emitter.instruction("mov rcx, QWORD PTR [rax - 8]");                        // load the kind word (value_type is byte 1)
    emitter.instruction("shr rcx, 8");                                          // shift the value_type byte into the low position
    emitter.instruction("and rcx, 0x7f");                                       // isolate the 7-bit value_type, excluding the COW flag in bit 15
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the value_type
    emitter.instruction("lea rcx, [rax + 24]");                                 // elements start right after the 24-byte header
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the elements base

    // -- nTableSize = next power of two, minimum 8 --
    emitter.instruction("mov r9, 8");                                           // start at the minimum packed table size
    emitter.label("__rt_zval_pack_array_packed_pow2");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length
    emitter.instruction("cmp r9, rcx");                                         // is the candidate size already >= length?
    emitter.instruction("jge __rt_zval_pack_array_packed_pow2_done");           // stop doubling once the table covers the length
    emitter.instruction("shl r9, 1");                                           // double the candidate size
    emitter.instruction("jmp __rt_zval_pack_array_packed_pow2");                // retry the power-of-two search
    emitter.label("__rt_zval_pack_array_packed_pow2_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save nTableSize

    // -- allocate the data block (8 sentinel bytes + nTableSize * 32-byte buckets) --
    emitter.instruction("mov rax, r9");                                         // copy nTableSize for the size computation
    emitter.instruction("shl rax, 5");                                          // nTableSize * 32 bytes of bucket storage
    emitter.instruction("add rax, 8");                                          // add the two 4-byte sentinel slots
    emitter.instruction("call __rt_heap_alloc");                                // rax = data block base
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the data block pointer
    emitter.instruction("mov DWORD PTR [rax], -1");                             // stamp the first packed sentinel (HT_INVALID_IDX)
    emitter.instruction("mov DWORD PTR [rax + 4], -1");                         // stamp the second packed sentinel
    emitter.instruction("lea rcx, [rax + 8]");                                  // arData = data block base + 8
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save arData
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // initialize the loop index i = 0

    // -- element loop: convert each elephc element into a bucket zval --
    emitter.label("__rt_zval_pack_array_packed_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the loop index
    emitter.instruction("cmp rax, rcx");                                        // has every element been processed?
    emitter.instruction("jge __rt_zval_pack_array_packed_loop_done");           // exit the loop once every element is packed

    // -- compute the element address: elements_base + i * elem_size --
    emitter.instruction("mov r14, QWORD PTR [rbp - 48]");                       // load the elements base
    emitter.instruction("mov r15, QWORD PTR [rbp - 24]");                       // load the element size
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the loop index
    emitter.instruction("imul rax, r15");                                       // i * elem_size (rdx:rax, but values fit in rax)
    emitter.instruction("add r14, rax");                                        // r14 = element address

    // -- dispatch on the value_type to stage (tag, lo, hi) for pack_element --
    emitter.instruction("mov r15, QWORD PTR [rbp - 32]");                       // reload the value_type
    emitter.instruction("cmp r15, 0");                                          // value_type 0 = int
    emitter.instruction("je __rt_zval_pack_array_packed_vt_int");               // stage the int element for pack_element
    emitter.instruction("cmp r15, 1");                                          // value_type 1 = string
    emitter.instruction("je __rt_zval_pack_array_packed_vt_str");               // stage the string element for pack_element
    emitter.instruction("cmp r15, 2");                                          // value_type 2 = float
    emitter.instruction("je __rt_zval_pack_array_packed_vt_float");             // stage the float element for pack_element
    emitter.instruction("cmp r15, 3");                                          // value_type 3 = bool
    emitter.instruction("je __rt_zval_pack_array_packed_vt_bool");              // stage the bool element for pack_element
    emitter.instruction("cmp r15, 7");                                          // value_type 7 = nested mixed cell
    emitter.instruction("je __rt_zval_pack_array_packed_vt_mixed");             // stage the nested Mixed cell for pack_element
    emitter.instruction("cmp r15, 4");                                          // value_type 4 = nested indexed array
    emitter.instruction("je __rt_zval_pack_array_packed_vt_idxarr");            // stage the nested indexed array for pack_element
    emitter.instruction("cmp r15, 5");                                          // value_type 5 = nested associative array
    emitter.instruction("je __rt_zval_pack_array_packed_vt_hasharr");           // stage the nested associative array for pack_element
    emitter.instruction("cmp r15, 11");                                         // value_type 11 = tagged scalar
    emitter.instruction("je __rt_zval_pack_array_packed_vt_tagged");            // stage the tagged scalar for pack_element
    emitter.instruction("jmp __rt_zval_pack_array_packed_vt_null");             // unknown kinds pack as null

    emitter.label("__rt_zval_pack_array_packed_vt_int");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = integer value
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("xor eax, eax");                                        // tag = 0 (int)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_float");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = float bits
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("mov eax, 2");                                          // tag = 2 (float)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_bool");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = bool payload
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("mov eax, 3");                                          // tag = 3 (bool)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_mixed");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = nested mixed cell pointer
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("mov eax, 7");                                          // tag = 7 (nested)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_idxarr");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = nested indexed-array pointer
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("mov eax, 4");                                          // tag = 4 (indexed array)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_hasharr");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = nested associative-array pointer
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("mov eax, 5");                                          // tag = 5 (associative array)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_tagged");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = tagged payload
    emitter.instruction("mov rax, QWORD PTR [r14 + 8]");                        // tag = tagged runtime tag
    emitter.instruction("xor rsi, rsi");                                        // hi = 0
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_str");
    emitter.instruction("mov rdi, QWORD PTR [r14]");                            // lo = string pointer
    emitter.instruction("mov rsi, QWORD PTR [r14 + 8]");                        // hi = string length
    emitter.instruction("mov eax, 1");                                          // tag = 1 (string)
    emitter.instruction("jmp __rt_zval_pack_array_packed_call_elem");           // pack the staged element
    emitter.label("__rt_zval_pack_array_packed_vt_null");
    emitter.instruction("mov eax, 8");                                          // tag = 8 (null)
    emitter.instruction("xor edi, edi");                                        // lo = 0
    emitter.instruction("xor esi, esi");                                        // hi = 0

    emitter.label("__rt_zval_pack_array_packed_call_elem");
    emitter.instruction("call __rt_zval_pack_element");                         // rax = freshly built child zval pointer

    // -- copy the 16-byte zval into the bucket at arData + i*32 --
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload arData
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload i
    emitter.instruction("shl r10, 5");                                          // i * 32 bytes per bucket
    emitter.instruction("lea r12, [r9 + r10]");                                 // r12 = bucket address
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the child zval value
    emitter.instruction("mov QWORD PTR [r12], rcx");                            // store the value into the bucket
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load the child zval type_info + u2
    emitter.instruction("mov QWORD PTR [r12 + 8], rcx");                        // store type_info + u2 into the bucket
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // reload i for the integer key
    emitter.instruction("mov QWORD PTR [r12 + 16], rcx");                       // bucket.h = i (packed integer key)
    emitter.instruction("mov QWORD PTR [r12 + 24], 0");                         // bucket.key = NULL (packed has no string key)

    // -- release the temporary child zval block (ownership transfers to the bucket) --
    emitter.instruction("call __rt_heap_free");                                 // free the 16-byte child zval block only
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload i
    emitter.instruction("inc r9");                                              // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // store the incremented index
    emitter.instruction("jmp __rt_zval_pack_array_packed_loop");                // continue the element loop

    emitter.label("__rt_zval_pack_array_packed_loop_done");

    // -- allocate and fill the 56-byte HashTable structure --
    emitter.instruction("mov rax, 56");                                         // HashTable structures are 56 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = HashTable pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the HashTable pointer
    emitter.instruction("mov DWORD PTR [rax], 1");                              // refcount = 1 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 4], 7");                          // gc type_info = IS_ARRAY (7)
    emitter.instruction("mov DWORD PTR [rax + 8], 96");                         // HASH_FLAG_PACKED | HASH_FLAG_STATIC_KEYS = 0x60
    emitter.instruction("mov DWORD PTR [rax + 12], -2");                        // nTableMask = HT_MIN_MASK = -2 (packed)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the data block base
    emitter.instruction("lea rcx, [rcx + 8]");                                  // arData = data block base + 8
    emitter.instruction("mov QWORD PTR [rax + 16], rcx");                       // store arData at offset 16
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length
    emitter.instruction("mov DWORD PTR [rax + 24], ecx");                       // store nNumUsed at offset 24 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 28], ecx");                       // store nNumOfElements at offset 28 (32-bit)
    emitter.instruction("mov DWORD PTR [rax + 32], -1");                        // nInternalPointer = HT_INVALID_IDX (32-bit)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the length for nNextFreeElement
    emitter.instruction("mov QWORD PTR [rax + 40], rcx");                       // store nNextFreeElement at offset 40 (64-bit)
    emitter.instruction("mov QWORD PTR [rax + 48], 0");                         // pDestructor = NULL
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // return the HashTable pointer
    emitter.instruction("add rsp, 96");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the zend_array pointer in rax
}
