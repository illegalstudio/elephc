//! Purpose:
//! Emits the `__rt_array_hash_union` runtime helper for indexed+associative array union.
//! Converts dense indexed keys to normalized integer hash keys before merging right hash entries.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - PHP array union uses one shared int/string key space; left indexed keys `0..len-1` must block matching right hash integer keys.

use elephc_memoir::values::packed_slots;

use elephc_memoir::values::hash_payload;
use elephc_memoir::hash_layout::{self, HeaderField};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_array_hash_union` runtime helper.
///
/// PHP array union: the left operand's indexed entries `0..len-1` occupy those
/// integer keys and block any matching integer keys from the right operand.
/// Right-side entries whose keys do not conflict are copied into the result.
///
/// ABI (ARM64): x0 = left indexed-array pointer, x1 = right hash pointer,
///              returns x0 = result hash pointer.
/// ABI (x86_64): rdi = left indexed-array pointer, rsi = right hash pointer,
///               returns rax = result hash pointer.
pub fn emit_array_hash_union(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_hash_union_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_hash_union ---");
    emitter.label_global("__rt_array_hash_union");

    // -- set up stack frame and allocate the destination hash --
    emitter.instruction("sub sp, sp, #128");                                    // reserve spill slots for indexed-to-hash union state
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish a stable frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the left indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the right associative-array pointer
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "x9", "x0"); // load the left indexed-array length
    hash_layout::emit_load_header_field(emitter, "x10", "x1", HeaderField::Count);
    emitter.instruction("str x9, [sp, #32]");                                   // save the left indexed-array length for the numeric-key loop
    emitter.instruction("add x11, x9, x10");                                    // estimate result entry count before applying duplicate-key filtering
    emitter.instruction("lsl x11, x11, #1");                                    // double the estimate to keep hash load factor below the growth threshold
    emitter.instruction("mov x0, #16");                                         // default to the minimum hash capacity
    emitter.instruction("cmp x11, x0");                                         // is the estimated capacity larger than the minimum?
    emitter.instruction("csel x0, x11, x0, hi");                                // choose max(estimated_capacity, 16)
    emitter.instruction("mov x1, #7");                                          // value_type 7 = mixed because cross-representation unions may merge heterogeneous values
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result associative-array hash table
    emitter.instruction("str x0, [sp, #16]");                                   // save the evolving result hash pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the left indexed-array pointer
    elephc_memoir::heap_kind::emit_load_metadata(emitter, "x10", "x9");
    emitter.instruction("lsr x10, x10, #8");                                    // move the left value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the left value_type tag without the persistent COW flag
    emitter.instruction("str x10, [sp, #40]");                                  // save the left value_type tag for copy dispatch
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the left indexed-array numeric key cursor

    // -- insert left indexed entries as integer-keyed hash entries --
    emitter.label("__rt_array_hash_union_left_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current left numeric key
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the left indexed-array length
    emitter.instruction("cmp x9, x10");                                         // have all left indexed entries been copied?
    emitter.instruction("b.ge __rt_array_hash_union_right_init");               // start merging the right hash after left entries are inserted
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the left value_type tag
    emitter.instruction("cmp x12, #1");                                         // is the left indexed array storing string slots?
    emitter.instruction("b.eq __rt_array_hash_union_left_string");              // strings need persistence before hash insertion
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the left indexed-array pointer
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(emitter, "x11", "x11");
    emitter.instruction("ldr x3, [x11, x9, lsl #3]");                           // load the scalar or pointer payload for this numeric key
    emitter.instruction("str x3, [sp, #48]");                                   // save the copied low payload word across optional retain calls
    emitter.instruction("str x12, [sp, #64]");                                  // save the runtime value tag for hash insertion
    emitter.instruction("cmp x12, #4");                                         // is the left payload in the refcounted range?
    emitter.instruction("b.lo __rt_array_hash_union_left_scalar");              // scalar payloads can be copied directly
    emitter.instruction("cmp x12, #7");                                         // is the left payload still a supported refcounted value?
    emitter.instruction("b.hi __rt_array_hash_union_left_scalar");              // unknown high tags fall back to scalar copying
    hash_payload::emit_retain_indexed_pointer_for_insert(emitter, 48, 64);
    emitter.instruction("b __rt_array_hash_union_left_insert");                 // insert the retained left payload

    emitter.label("__rt_array_hash_union_left_scalar");
    emitter.instruction("ldr x3, [sp, #48]");                                   // reload the scalar left payload low word
    emitter.instruction("mov x4, xzr");                                         // scalar copied values do not use a high payload word here
    emitter.instruction("ldr x5, [sp, #64]");                                   // reload the scalar runtime value tag
    emitter.instruction("b __rt_array_hash_union_left_insert");                 // insert the scalar left payload

    emitter.label("__rt_array_hash_union_left_string");
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the left indexed-array pointer
    emitter.instruction("lsl x13, x9, #4");                                     // compute the byte offset for a 16-byte string slot
    emitter.instruction("add x11, x11, x13");                                   // advance to the selected string slot
    packed_slots::emit_packed_payload_aarch64(emitter, "x11", "x11");
    hash_payload::emit_persist_indexed_pair_for_insert(emitter);

    emitter.label("__rt_array_hash_union_left_insert");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current result hash pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // use the indexed position as a normalized integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks the key as integer
    emitter.instruction("bl __rt_hash_set");                                    // insert the left entry into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // save the possibly grown result hash pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left numeric key cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next left indexed key
    emitter.instruction("str x9, [sp, #24]");                                   // save the advanced left cursor
    emitter.instruction("b __rt_array_hash_union_left_loop");                   // continue converting left indexed entries

    // -- walk right hash entries and copy only keys missing from the left/result --
    emitter.label("__rt_array_hash_union_right_init");
    emitter.instruction("str xzr, [sp, #24]");                                  // reset cursor to the start of the right insertion-order list
    emitter.label("__rt_array_hash_union_right_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right associative-array pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the right insertion-order iterator cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // fetch the next right entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did the iterator report the terminal sentinel?
    emitter.instruction("b.eq __rt_array_hash_union_done");                     // finish once every right entry has been considered
    emitter.instruction("str x0, [sp, #24]");                                   // save the next right iterator cursor
    emitter.instruction("str x1, [sp, #72]");                                   // save the borrowed right key low word
    emitter.instruction("str x2, [sp, #80]");                                   // save the borrowed right key high word
    emitter.instruction("str x3, [sp, #88]");                                   // save the borrowed right value low word
    emitter.instruction("str x4, [sp, #96]");                                   // save the borrowed right value high word
    emitter.instruction("str x5, [sp, #104]");                                  // save the right value runtime tag
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current result hash pointer
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload the candidate right key low word
    emitter.instruction("ldr x2, [sp, #80]");                                   // reload the candidate right key high word
    emitter.instruction("bl __rt_hash_get");                                    // test whether the left/result already contains this logical key
    emitter.instruction("cbnz x0, __rt_array_hash_union_right_loop");           // duplicate keys keep the left value and skip insertion
    emitter.instruction("ldr x5, [sp, #104]");                                  // reload the right value runtime tag
    emitter.instruction("cmp x5, #1");                                          // is the right value a string payload?
    emitter.instruction("b.eq __rt_array_hash_union_right_string");             // strings must be persisted for the result hash owner
    emitter.instruction("cmp x5, #4");                                          // is the right value in the refcounted payload range?
    emitter.instruction("b.lo __rt_array_hash_union_right_scalar");             // scalar payloads can be copied directly
    emitter.instruction("cmp x5, #7");                                          // is the right value still a supported refcounted value?
    emitter.instruction("b.hi __rt_array_hash_union_right_scalar");             // unknown high tags fall back to scalar copying
    hash_payload::HashPayloadSlots { low: 88, reload_later: false }.emit_retain_preserving_words(emitter);
    emitter.instruction("b __rt_array_hash_union_right_insert");                // insert the retained right payload

    emitter.label("__rt_array_hash_union_right_string");
    hash_payload::emit_persist_staged_string_for_insert(emitter, 88);
    emitter.instruction("b __rt_array_hash_union_right_insert");                // insert the owned right string payload

    emitter.label("__rt_array_hash_union_right_scalar");
    emitter.instruction("ldr x3, [sp, #88]");                                   // reload the scalar right value low word
    emitter.instruction("ldr x4, [sp, #96]");                                   // reload the scalar right value high word
    emitter.instruction("ldr x5, [sp, #104]");                                  // reload the scalar runtime value tag

    emitter.label("__rt_array_hash_union_right_insert");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current result hash pointer
    emitter.instruction("ldr x1, [sp, #72]");                                   // reload the missing right key low word
    emitter.instruction("ldr x2, [sp, #80]");                                   // reload the missing right key high word
    emitter.instruction("bl __rt_hash_set");                                    // append the missing right entry to the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // save the possibly grown result hash pointer
    emitter.instruction("b __rt_array_hash_union_right_loop");                  // continue scanning right-side entries

    emitter.label("__rt_array_hash_union_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the completed result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the union spill slots
    emitter.instruction("ret");                                                 // return to generated code
}

/// x86_64/Linux variant of `emit_array_hash_union`.
/// Uses the System V AMD64 ABI: rdi = left indexed-array, rsi = right hash,
/// returns rax = result hash.
fn emit_array_hash_union_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_hash_union ---");
    emitter.label_global("__rt_array_hash_union");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving union spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for indexed-to-hash union state
    emitter.instruction("sub rsp, 112");                                        // reserve local storage while keeping nested calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right associative-array pointer
    elephc_memoir::values::packed_slots::emit_load_length(emitter, "r10", "rdi"); // load the left indexed-array length
    hash_layout::emit_load_header_field(emitter, "r11", "rsi", HeaderField::Count);
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the left indexed-array length for the numeric-key loop
    emitter.instruction("lea rdi, [r10 + r11]");                                // estimate result entry count before duplicate-key filtering
    emitter.instruction("shl rdi, 1");                                          // double the estimate to keep hash load factor below the growth threshold
    emitter.instruction("cmp rdi, 16");                                         // is the estimated capacity at least the minimum hash capacity?
    emitter.instruction("jae __rt_array_hash_union_x86_capacity_ready");        // keep the estimate when it is large enough
    emitter.instruction("mov rdi, 16");                                         // otherwise use the minimum hash capacity
    emitter.label("__rt_array_hash_union_x86_capacity_ready");
    emitter.instruction("mov rsi, 7");                                          // value_type 7 = mixed because cross-representation unions may merge heterogeneous values
    emitter.instruction("call __rt_hash_new");                                  // allocate the result associative-array hash table
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the evolving result hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the left indexed-array pointer
    elephc_memoir::heap_kind::emit_load_metadata(emitter, "r11", "r10");
    elephc_memoir::values::array_header::emit_extract_value_tag_x86_64(emitter, "r11", "r11");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the left value_type tag for copy dispatch
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the left indexed-array numeric key cursor

    emitter.label("__rt_array_hash_union_x86_left_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current left numeric key
    emitter.instruction("cmp r10, QWORD PTR [rbp - 40]");                       // have all left indexed entries been copied?
    emitter.instruction("jge __rt_array_hash_union_x86_right_init");            // start merging the right hash after left entries are inserted
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the left value_type tag
    emitter.instruction("cmp r11, 1");                                          // is the left indexed array storing string slots?
    emitter.instruction("je __rt_array_hash_union_x86_left_string");            // strings need persistence before hash insertion
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left indexed-array pointer
    elephc_memoir::values::packed_slots::emit_load_indexed_word(emitter, "rax", "r10", "rcx", "r11");
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the copied low payload word across optional retain calls
    emitter.instruction("cmp r11, 4");                                          // is the left payload in the refcounted range?
    emitter.instruction("jb __rt_array_hash_union_x86_left_scalar");            // scalar payloads can be copied directly
    emitter.instruction("cmp r11, 7");                                          // is the left payload still a supported refcounted value?
    emitter.instruction("ja __rt_array_hash_union_x86_left_scalar");            // unknown high tags fall back to scalar copying
    hash_payload::emit_retain_indexed_pointer_for_insert(emitter, 56, 48);
    emitter.instruction("jmp __rt_array_hash_union_x86_left_insert");           // insert the retained left payload

    emitter.label("__rt_array_hash_union_x86_left_scalar");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the scalar left payload low word
    emitter.instruction("xor r8d, r8d");                                        // scalar copied values do not use a high payload word here
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the scalar runtime value tag
    emitter.instruction("jmp __rt_array_hash_union_x86_left_insert");           // insert the scalar left payload

    emitter.label("__rt_array_hash_union_x86_left_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current left numeric key
    emitter.instruction("shl r10, 4");                                          // compute the byte offset for a 16-byte string slot
    elephc_memoir::values::packed_slots::emit_packed_byte_offset_address(emitter, "r11", "rax", "r10");
    hash_payload::emit_persist_indexed_pair_for_insert(emitter);

    emitter.label("__rt_array_hash_union_x86_left_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // use the indexed position as a normalized integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks the key as integer
    emitter.instruction("call __rt_hash_set");                                  // insert the left entry into the result hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the possibly grown result hash pointer
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next left indexed key
    emitter.instruction("jmp __rt_array_hash_union_x86_left_loop");             // continue converting left indexed entries

    emitter.label("__rt_array_hash_union_x86_right_init");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // reset cursor to the start of the right insertion-order list

    emitter.label("__rt_array_hash_union_x86_right_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right associative-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the right insertion-order iterator cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // fetch the next right entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did the iterator report the terminal sentinel?
    emitter.instruction("je __rt_array_hash_union_x86_done");                   // finish once every right entry has been considered
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next right iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // save the borrowed right key low word
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save the borrowed right key high word
    emitter.instruction("mov QWORD PTR [rbp - 88], rcx");                       // save the borrowed right value low word
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                        // save the borrowed right value high word
    emitter.instruction("mov QWORD PTR [rbp - 104], r9");                       // save the right value runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload the candidate right key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload the candidate right key high word
    emitter.instruction("call __rt_hash_get");                                  // test whether the left/result already contains this logical key
    emitter.instruction("test rax, rax");                                       // did the lookup find a left-side value?
    emitter.instruction("jnz __rt_array_hash_union_x86_right_loop");            // duplicate keys keep the left value and skip insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // reload the right value runtime tag
    emitter.instruction("cmp r10, 1");                                          // is the right value a string payload?
    emitter.instruction("je __rt_array_hash_union_x86_right_string");           // strings must be persisted for the result hash owner
    emitter.instruction("cmp r10, 4");                                          // is the right value in the refcounted payload range?
    emitter.instruction("jb __rt_array_hash_union_x86_right_scalar");           // scalar payloads can be copied directly
    emitter.instruction("cmp r10, 7");                                          // is the right value still a supported refcounted value?
    emitter.instruction("ja __rt_array_hash_union_x86_right_scalar");           // unknown high tags fall back to scalar copying
    hash_payload::HashPayloadSlots { low: 88, reload_later: false }.emit_retain_preserving_words(emitter);
    emitter.instruction("jmp __rt_array_hash_union_x86_right_insert");          // insert the retained right payload

    emitter.label("__rt_array_hash_union_x86_right_string");
    hash_payload::emit_persist_staged_string_for_insert(emitter, 88);
    emitter.instruction("jmp __rt_array_hash_union_x86_right_insert");          // insert the owned right string payload

    emitter.label("__rt_array_hash_union_x86_right_scalar");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 88]");                       // reload the scalar right value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                        // reload the scalar right value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // reload the scalar runtime value tag

    emitter.label("__rt_array_hash_union_x86_right_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload the missing right key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload the missing right key high word
    emitter.instruction("call __rt_hash_set");                                  // append the missing right entry to the result hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the possibly grown result hash pointer
    emitter.instruction("jmp __rt_array_hash_union_x86_right_loop");            // continue scanning right-side entries

    emitter.label("__rt_array_hash_union_x86_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the completed result hash pointer
    emitter.instruction("add rsp, 112");                                        // release the union spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
