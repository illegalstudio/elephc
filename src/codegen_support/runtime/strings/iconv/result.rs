//! Purpose:
//! Emits `__rt_iconv_materialize` and `__rt_iconv_build_array`, which turn one bridge
//! result block into the PHP value the caller stores.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::iconv::emit_iconv()`.
//!
//! Key details:
//! - Every iconv builtin except `iconv_set_encoding()` has a union return, so the result
//!   is always handed back as a boxed Mixed cell.
//! - String payloads are copied into runtime-owned storage with `__rt_str_persist` before
//!   boxing, because the bridge frees its own buffers right afterwards.
//! - The boxed cell is stamped by hand rather than through `__rt_mixed_from_value`, which
//!   persists strings and retains containers again: both payloads here are already owned,
//!   so a second copy or reference would leak on every call.
//! - The packed array format is length-prefixed throughout:
//!   `[count]` then `[key_len][key][value_count]` and `[len][bytes]` per value. A value
//!   count of one is a string entry; anything else becomes a PHP list.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::call::{RESULT_BYTES, RESULT_INT, RESULT_KIND, RESULT_LEN};

/// Uniform heap-header kind reserved for boxed Mixed cells.
const MIXED_HEAP_KIND: u32 = 5;

/// Returns the x86_64 heap-kind word that marks an allocation as a boxed Mixed cell.
fn mixed_heap_kind_word() -> u64 {
    crate::codegen_support::sentinels::x86_64_heap_kind_word(MIXED_HEAP_KIND)
}

/// Emits the result materializer and its packed-array decoder for the active target.
pub(super) fn emit_iconv_result(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_materialize_x86_64(emitter);
        emit_build_array_x86_64(emitter);
        return;
    }
    emit_materialize_aarch64(emitter);
    emit_build_array_aarch64(emitter);
}

/// Emits the AArch64 `__rt_iconv_materialize` helper.
///
/// Input:  x0 = result block pointer.
/// Output: x0 = boxed Mixed cell holding the PHP value the operation produced.
fn emit_materialize_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv materialize ---");
    emitter.label_global("__rt_iconv_materialize");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame for the nested boxing calls
    emitter.instruction(&format!("ldr x9, [x0, #{}]", RESULT_KIND));            // load the outcome kind the bridge reported
    emitter.instruction("cmp x9, #1");                                          // kind 1 carries an integer payload
    emitter.instruction("b.eq __rt_iconv_materialize_int");                     // box the integer result
    emitter.instruction("cmp x9, #2");                                          // kind 2 carries a byte-string payload
    emitter.instruction("b.eq __rt_iconv_materialize_string");                  // box the string result
    emitter.instruction("cmp x9, #3");                                          // kind 3 is PHP true
    emitter.instruction("b.eq __rt_iconv_materialize_true");                    // box the boolean true result
    emitter.instruction("cmp x9, #4");                                          // kind 4 carries a packed associative array
    emitter.instruction("b.eq __rt_iconv_materialize_array");                   // build and box the array result
    emitter.instruction("mov x1, #0");                                          // PHP false payload is a zero boolean
    emitter.instruction("b __rt_iconv_materialize_bool");                       // share the boolean boxing tail

    emitter.label("__rt_iconv_materialize_true");
    emitter.instruction("mov x1, #1");                                          // PHP true payload is a one boolean
    emitter.label("__rt_iconv_materialize_bool");
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = boolean
    emitter.instruction("mov x2, #0");                                          // boolean Mixed payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the boolean outcome
    emitter.instruction("b __rt_iconv_materialize_done");                       // return the boxed boolean

    emitter.label("__rt_iconv_materialize_int");
    emitter.instruction(&format!("ldr x1, [x0, #{}]", RESULT_INT));             // load the integer the bridge computed
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, #0");                                          // integer Mixed payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer outcome
    emitter.instruction("b __rt_iconv_materialize_done");                       // return the boxed integer

    emitter.label("__rt_iconv_materialize_string");
    emitter.instruction(&format!("ldr x2, [x0, #{}]", RESULT_LEN));             // load the produced byte length
    emitter.instruction(&format!("ldr x1, [x0, #{}]", RESULT_BYTES));           // load the bridge-owned byte pointer
    emitter.instruction("bl __rt_str_persist");                                 // copy the bytes into runtime-owned storage
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // hold the owned string while the Mixed cell is allocated
    emitter.instruction("mov x0, #24");                                         // a Mixed cell is a tag word plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed cell
    emitter.instruction("mov x9, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a Mixed cell
    emitter.instruction("mov x9, #1");                                          // runtime tag 1 = string
    emitter.instruction("str x9, [x0]");                                        // store the runtime value tag
    emitter.instruction("ldp x9, x10, [sp], #16");                              // reload the owned string pointer and length
    emitter.instruction("stp x9, x10, [x0, #8]");                               // install the owned string as the cell payload
    emitter.instruction("b __rt_iconv_materialize_done");                       // return the boxed string

    emitter.label("__rt_iconv_materialize_array");
    emitter.instruction("bl __rt_iconv_build_array");                           // decode the packed entries into a runtime hash
    emitter.instruction("str x0, [sp, #-16]!");                                 // hold the owned hash while the Mixed cell is allocated
    emitter.instruction("mov x0, #24");                                         // a Mixed cell is a tag word plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed cell
    emitter.instruction("mov x9, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a Mixed cell
    emitter.instruction("mov x9, #5");                                          // runtime tag 5 = associative array
    emitter.instruction("str x9, [x0]");                                        // store the runtime value tag
    emitter.instruction("ldr x9, [sp], #16");                                   // reload the owned hash pointer
    emitter.instruction("str x9, [x0, #8]");                                    // install the owned hash as the cell payload
    emitter.instruction("str xzr, [x0, #16]");                                  // array Mixed payloads do not use a high word

    emitter.label("__rt_iconv_materialize_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame pointer and return address
    emitter.instruction("ret");                                                 // return the boxed Mixed value
}

/// Emits the AArch64 `__rt_iconv_build_array` packed-entry decoder.
///
/// Input:  x0 = result block pointer.
/// Output: x0 = runtime hash whose values are strings or nested string lists.
fn emit_build_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv build array ---");
    emitter.label_global("__rt_iconv_build_array");

    // -- frame: [sp,#0]=cursor [sp,#8]=hash [sp,#16]=entries [sp,#24]=key ptr --
    // -- [sp,#32]=key len [sp,#40]=values left [sp,#48]=nested list --
    emitter.instruction("sub sp, sp, #96");                                     // reserve the packed-entry decoder state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable decoder frame
    emitter.instruction(&format!("ldr x9, [x0, #{}]", RESULT_BYTES));           // load the packed payload pointer
    emitter.instruction("mov x10, #0");                                         // an absent payload decodes to zero entries
    emitter.instruction("cbz x9, __rt_iconv_array_start");                      // skip the header read when nothing was produced
    emitter.instruction("ldr x10, [x9]");                                       // read the entry count from the packed header
    emitter.instruction("add x9, x9, #8");                                      // advance past the entry count
    emitter.label("__rt_iconv_array_start");
    emitter.instruction("str x9, [sp, #0]");                                    // publish the decode cursor
    emitter.instruction("str x10, [sp, #16]");                                  // publish the number of entries still to decode
    emitter.instruction("mov x0, x10");                                         // size the hash for exactly the entries it will hold
    emitter.instruction("mov x1, #7");                                          // hash value type 7 = heterogeneous Mixed values
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // publish the result hash across insertions

    emitter.label("__rt_iconv_array_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the number of entries still to decode
    emitter.instruction("cbz x9, __rt_iconv_array_done");                       // every entry has been inserted
    emitter.instruction("sub x9, x9, #1");                                      // consume one entry
    emitter.instruction("str x9, [sp, #16]");                                   // publish the reduced entry count
    emitter.instruction("ldr x9, [sp, #0]");                                    // load the decode cursor
    emitter.instruction("ldr x10, [x9]");                                       // read this entry's key length
    emitter.instruction("add x9, x9, #8");                                      // advance past the key length
    emitter.instruction("str x10, [sp, #32]");                                  // publish the key length
    emitter.instruction("str x9, [sp, #24]");                                   // publish the key pointer
    emitter.instruction("add x9, x9, x10");                                     // advance past the key bytes
    emitter.instruction("ldr x10, [x9]");                                       // read how many values this key carries
    emitter.instruction("add x9, x9, #8");                                      // advance past the value count
    emitter.instruction("str x10, [sp, #40]");                                  // publish the value count
    emitter.instruction("str x9, [sp, #0]");                                    // publish the advanced decode cursor
    emitter.instruction("cmp x10, #1");                                         // a single value is stored as a plain string
    emitter.instruction("b.ne __rt_iconv_array_list");                          // repeated field names become a PHP list

    emitter.instruction("ldr x9, [sp, #0]");                                    // load the decode cursor at the value header
    emitter.instruction("ldr x2, [x9]");                                        // read the value byte length
    emitter.instruction("add x9, x9, #8");                                      // advance past the value length
    emitter.instruction("mov x1, x9");                                          // the value bytes start right after the length
    emitter.instruction("add x9, x9, x2");                                      // advance past the value bytes
    emitter.instruction("str x9, [sp, #0]");                                    // publish the advanced decode cursor
    emitter.instruction("bl __rt_str_persist");                                 // copy the value into runtime-owned storage
    emitter.instruction("mov x3, x1");                                          // hash value low word = owned string pointer
    emitter.instruction("mov x4, x2");                                          // hash value high word = string length
    emitter.instruction("mov x5, #1");                                          // runtime tag 1 = string value
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the result hash
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the key length
    emitter.instruction("bl __rt_hash_set");                                    // insert the string entry
    emitter.instruction("str x0, [sp, #8]");                                    // retain the possibly-grown result hash
    emitter.instruction("b __rt_iconv_array_loop");                             // continue with the next entry

    emitter.label("__rt_iconv_array_list");
    emitter.instruction("ldr x0, [sp, #40]");                                   // size the list for exactly its value count
    emitter.instruction("mov x1, #16");                                         // string array element size = pointer plus length
    emitter.instruction("bl __rt_array_new");                                   // allocate the nested value list
    emitter.instruction("str x0, [sp, #48]");                                   // publish the nested list across pushes

    emitter.label("__rt_iconv_array_values");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load the number of values still to decode
    emitter.instruction("cbz x9, __rt_iconv_array_values_done");                // every value has been pushed
    emitter.instruction("sub x9, x9, #1");                                      // consume one value
    emitter.instruction("str x9, [sp, #40]");                                   // publish the reduced value count
    emitter.instruction("ldr x9, [sp, #0]");                                    // load the decode cursor at the value header
    emitter.instruction("ldr x2, [x9]");                                        // read the value byte length
    emitter.instruction("add x9, x9, #8");                                      // advance past the value length
    emitter.instruction("mov x1, x9");                                          // the value bytes start right after the length
    emitter.instruction("add x9, x9, x2");                                      // advance past the value bytes
    emitter.instruction("str x9, [sp, #0]");                                    // publish the advanced decode cursor
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the nested list
    emitter.instruction("bl __rt_array_push_str");                              // append the persisted value string
    emitter.instruction("str x0, [sp, #48]");                                   // retain the possibly-grown nested list
    emitter.instruction("b __rt_iconv_array_values");                           // continue with the next value

    emitter.label("__rt_iconv_array_values_done");
    emitter.instruction("ldr x3, [sp, #48]");                                   // hash value low word = nested list pointer
    emitter.instruction("mov x4, #0");                                          // list Mixed payloads do not use a high word
    emitter.instruction("mov x5, #4");                                          // runtime tag 4 = indexed array value
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the result hash
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the key length
    emitter.instruction("bl __rt_hash_set");                                    // insert the list entry
    emitter.instruction("str x0, [sp, #8]");                                    // retain the possibly-grown result hash
    emitter.instruction("b __rt_iconv_array_loop");                             // continue with the next entry

    emitter.label("__rt_iconv_array_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed result hash
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the decoder frame
    emitter.instruction("ret");                                                 // return the runtime hash
}

/// Emits the Linux x86_64 `__rt_iconv_materialize` helper.
///
/// Input:  rdi = result block pointer.
/// Output: rax = boxed Mixed cell holding the PHP value the operation produced.
fn emit_materialize_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv materialize ---");
    emitter.label_global("__rt_iconv_materialize");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned frame for the nested boxing calls
    emitter.instruction("sub rsp, 16");                                         // keep the nested boxing calls 16-byte aligned
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", RESULT_KIND)); // load the outcome kind the bridge reported
    emitter.instruction("cmp r10, 1");                                          // kind 1 carries an integer payload
    emitter.instruction("je __rt_iconv_materialize_int_linux_x86_64");          // box the integer result
    emitter.instruction("cmp r10, 2");                                          // kind 2 carries a byte-string payload
    emitter.instruction("je __rt_iconv_materialize_string_linux_x86_64");       // box the string result
    emitter.instruction("cmp r10, 3");                                          // kind 3 is PHP true
    emitter.instruction("je __rt_iconv_materialize_true_linux_x86_64");         // box the boolean true result
    emitter.instruction("cmp r10, 4");                                          // kind 4 carries a packed associative array
    emitter.instruction("je __rt_iconv_materialize_array_linux_x86_64");        // build and box the array result
    emitter.instruction("xor edi, edi");                                        // PHP false payload is a zero boolean
    emitter.instruction("jmp __rt_iconv_materialize_bool_linux_x86_64");        // share the boolean boxing tail

    emitter.label("__rt_iconv_materialize_true_linux_x86_64");
    emitter.instruction("mov edi, 1");                                          // PHP true payload is a one boolean
    emitter.label("__rt_iconv_materialize_bool_linux_x86_64");
    emitter.instruction("xor esi, esi");                                        // boolean Mixed payloads do not use a high word
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = boolean
    emitter.instruction("call __rt_mixed_from_value");                          // box the boolean outcome
    emitter.instruction("jmp __rt_iconv_materialize_done_linux_x86_64");        // return the boxed boolean

    emitter.label("__rt_iconv_materialize_int_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rdi + {}]", RESULT_INT)); // load the integer the bridge computed
    emitter.instruction("xor esi, esi");                                        // integer Mixed payloads do not use a high word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer outcome
    emitter.instruction("jmp __rt_iconv_materialize_done_linux_x86_64");        // return the boxed integer

    emitter.label("__rt_iconv_materialize_string_linux_x86_64");
    emitter.instruction(&format!("mov rdx, QWORD PTR [rdi + {}]", RESULT_LEN)); // load the produced byte length
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", RESULT_BYTES)); // load the bridge-owned byte pointer into the string register pair
    emitter.instruction("call __rt_str_persist");                               // copy the bytes into runtime-owned storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // hold the owned string pointer across the allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // hold the owned string length across the allocation
    emitter.instruction("mov rax, 24");                                         // a Mixed cell is a tag word plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", mixed_heap_kind_word()));   // stamp the canonical x86_64 heap-kind word for a Mixed cell
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the Mixed-cell heap kind
    emitter.instruction("mov QWORD PTR [rax], 1");                              // runtime tag 1 = string
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned string pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // install the owned string pointer as the payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the owned string length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // install the string length as the payload high word
    emitter.instruction("jmp __rt_iconv_materialize_done_linux_x86_64");        // return the boxed string

    emitter.label("__rt_iconv_materialize_array_linux_x86_64");
    emitter.instruction("call __rt_iconv_build_array");                         // decode the packed entries into a runtime hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // hold the owned hash across the allocation
    emitter.instruction("mov rax, 24");                                         // a Mixed cell is a tag word plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", mixed_heap_kind_word()));   // stamp the canonical x86_64 heap-kind word for a Mixed cell
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the Mixed-cell heap kind
    emitter.instruction("mov QWORD PTR [rax], 5");                              // runtime tag 5 = associative array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned hash pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // install the owned hash as the cell payload
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // array Mixed payloads do not use a high word

    emitter.label("__rt_iconv_materialize_done_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the materializer frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed value
}

/// Emits the Linux x86_64 `__rt_iconv_build_array` packed-entry decoder.
///
/// Input:  rdi = result block pointer.
/// Output: rax = runtime hash whose values are strings or nested string lists.
fn emit_build_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iconv build array ---");
    emitter.label_global("__rt_iconv_build_array");

    // -- frame: [rbp-8]=cursor [rbp-16]=hash [rbp-24]=entries [rbp-32]=key ptr --
    // -- [rbp-40]=key len [rbp-48]=values left [rbp-56]=nested list --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the decoder state
    emitter.instruction("sub rsp, 64");                                         // reserve the packed-entry decoder state
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", RESULT_BYTES)); // load the packed payload pointer
    emitter.instruction("xor r11d, r11d");                                      // an absent payload decodes to zero entries
    emitter.instruction("test r10, r10");                                       // is there a packed payload at all?
    emitter.instruction("jz __rt_iconv_array_start_linux_x86_64");              // skip the header read when nothing was produced
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // read the entry count from the packed header
    emitter.instruction("add r10, 8");                                          // advance past the entry count
    emitter.label("__rt_iconv_array_start_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // publish the decode cursor
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // publish the number of entries still to decode
    emitter.instruction("mov rdi, r11");                                        // size the hash for exactly the entries it will hold
    emitter.instruction("mov esi, 7");                                          // hash value type 7 = heterogeneous Mixed values
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // publish the result hash across insertions

    emitter.label("__rt_iconv_array_loop_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // load the number of entries still to decode
    emitter.instruction("test r10, r10");                                       // is every entry inserted?
    emitter.instruction("jz __rt_iconv_array_done_linux_x86_64");               // finish once no entry is left
    emitter.instruction("sub r10, 1");                                          // consume one entry
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // publish the reduced entry count
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the decode cursor
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // read this entry's key length
    emitter.instruction("add r10, 8");                                          // advance past the key length
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the key length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // publish the key pointer
    emitter.instruction("add r10, r11");                                        // advance past the key bytes
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // read how many values this key carries
    emitter.instruction("add r10, 8");                                          // advance past the value count
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // publish the value count
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // publish the advanced decode cursor
    emitter.instruction("cmp r11, 1");                                          // a single value is stored as a plain string
    emitter.instruction("jne __rt_iconv_array_list_linux_x86_64");              // repeated field names become a PHP list

    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the decode cursor at the value header
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // read the value byte length
    emitter.instruction("add r10, 8");                                          // advance past the value length
    emitter.instruction("mov rax, r10");                                        // the value bytes start right after the length
    emitter.instruction("add r10, rdx");                                        // advance past the value bytes
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // publish the advanced decode cursor
    emitter.instruction("call __rt_str_persist");                               // copy the value into runtime-owned storage
    emitter.instruction("mov rcx, rax");                                        // hash value low word = owned string pointer
    emitter.instruction("mov r8, rdx");                                         // hash value high word = string length
    emitter.instruction("mov r9d, 1");                                          // runtime tag 1 = string value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the result hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the key length
    emitter.instruction("call __rt_hash_set");                                  // insert the string entry
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // retain the possibly-grown result hash
    emitter.instruction("jmp __rt_iconv_array_loop_linux_x86_64");              // continue with the next entry

    emitter.label("__rt_iconv_array_list_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // size the list for exactly its value count
    emitter.instruction("mov esi, 16");                                         // string array element size = pointer plus length
    emitter.instruction("call __rt_array_new");                                 // allocate the nested value list
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // publish the nested list across pushes

    emitter.label("__rt_iconv_array_values_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load the number of values still to decode
    emitter.instruction("test r10, r10");                                       // is every value pushed?
    emitter.instruction("jz __rt_iconv_array_values_done_linux_x86_64");        // finish once no value is left
    emitter.instruction("sub r10, 1");                                          // consume one value
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // publish the reduced value count
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the decode cursor at the value header
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // read the value byte length
    emitter.instruction("add r10, 8");                                          // advance past the value length
    emitter.instruction("mov rsi, r10");                                        // the value bytes start right after the length
    emitter.instruction("add r10, rdx");                                        // advance past the value bytes
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // publish the advanced decode cursor
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the nested list
    emitter.instruction("call __rt_array_push_str");                            // append the persisted value string
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // retain the possibly-grown nested list
    emitter.instruction("jmp __rt_iconv_array_values_linux_x86_64");            // continue with the next value

    emitter.label("__rt_iconv_array_values_done_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // hash value low word = nested list pointer
    emitter.instruction("xor r8d, r8d");                                        // list Mixed payloads do not use a high word
    emitter.instruction("mov r9d, 4");                                          // runtime tag 4 = indexed array value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the result hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the key length
    emitter.instruction("call __rt_hash_set");                                  // insert the list entry
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // retain the possibly-grown result hash
    emitter.instruction("jmp __rt_iconv_array_loop_linux_x86_64");              // continue with the next entry

    emitter.label("__rt_iconv_array_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed result hash
    emitter.instruction("mov rsp, rbp");                                        // release the decoder frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the runtime hash
}
