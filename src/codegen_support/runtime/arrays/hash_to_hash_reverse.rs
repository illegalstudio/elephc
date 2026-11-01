//! Purpose:
//! Emits `__rt_hash_to_hash_reverse`, backing `array_reverse($assoc, true)` when the argument is
//! already a hash.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `__rt_array_to_hash_reverse` walks fixed-size payload slots, which a hash does not have, so
//!   `array_reverse($assoc, true)` refused to compile at all
//!   (`unsupported EIR backend feature: array_reverse preserve_keys for PHP type AssocArray`).
//!   That became a build failure for ordinary PHP the moment a bare `array` contract started
//!   resolving to the hash, which is how Symfony's compile broke.
//! - A hash IS a doubly linked insertion-order list -- `head` at header[24], `tail` at header[32],
//!   and `prev`/`next` at entry offsets 48/56 -- so the reverse walk is the forward walk with the
//!   two links swapped. No temporary buffer, no second pass, still linear.
//! - The entry chain is followed directly rather than through `__rt_hash_iter_next`, which only
//!   walks forward. The cursor is advanced to `prev` BEFORE any call, because every helper called
//!   per entry clobbers the caller-saved registers the chain pointer would live in.
//! - Payload ownership mirrors `__rt_hash_to_hash_unique` exactly: strings are persisted into
//!   independent copies, heap-backed values (tags 4..7) are retained, scalars are copied as-is.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// hash_to_hash_reverse: build an owned hash holding the source entries in reverse insertion
/// order, each under its ORIGINAL key.
///
/// Input:  `x0` / `rdi` = source hash pointer
/// Output: `x0` / `rax` = new owned hash
pub fn emit_hash_to_hash_reverse(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_to_hash_reverse_x86_64(emitter);
        return;
    }
    emit_hash_to_hash_reverse_aarch64(emitter);
}

/// Emits `__rt_hash_to_hash_reverse` for ARM64.
///
/// Frame (96 bytes): `[0]` source, `[8]` result, `[16]` cursor slot index, `[24]` key pointer,
/// `[32]` key length, `[40]` value low, `[48]` value high, `[56]` value tag, `[64]` value_type.
fn emit_hash_to_hash_reverse_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_to_hash_reverse ---");
    emitter.label_global("__rt_hash_to_hash_reverse");
    emitter.instruction("sub sp, sp, #96");                                     // reserve the walk state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the frame pointer

    emitter.instruction("str x0, [sp, #0]");                                    // save the source hash
    emitter.instruction("cbz x0, __rt_h2h_rev_empty");                          // a null table has nothing to reverse
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "x7",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp x0, x7");                                          // does the table carry the in-band null-container sentinel?
    emitter.instruction("b.eq __rt_h2h_rev_empty");                             // sentinel-null tables have nothing to reverse

    emitter.instruction("ldr x9, [x0]");                                        // entry count, used as the capacity hint
    emitter.instruction("ldr x10, [x0, #16]");                                  // the hash header's value_type
    emitter.instruction("str x10, [sp, #64]");                                  // save it: payload ownership depends on it
    emitter.instruction("cmp x9, #8");                                          // below the minimum useful capacity?
    emitter.instruction("b.ge __rt_h2h_rev_cap_ok");                            // use the count as the hint
    emitter.instruction("mov x9, #8");                                          // clamp small sources to a floor
    emitter.label("__rt_h2h_rev_cap_ok");
    emitter.instruction("mov x0, x9");                                          // capacity hint
    emitter.instruction("mov x1, x10");                                         // the result carries the source's value_type
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source
    emitter.instruction("ldr x11, [x0, #32]");                                  // header[32]: TAIL slot index starts the reverse walk
    emitter.instruction("str x11, [sp, #16]");                                  // park the cursor

    emitter.label("__rt_h2h_rev_loop");
    emitter.instruction("ldr x11, [sp, #16]");                                  // current slot index
    emitter.instruction("cmp x11, #-1");                                        // has the walk run off the head?
    emitter.instruction("b.eq __rt_h2h_rev_done");                              // walk finished: return the result
    emitter.instruction("ldr x0, [sp, #0]");                                    // the source hash base
    super::hash_layout::emit_entry_address(emitter, "x8", "x0", "x11");
    emitter.instruction("ldr x12, [x8, #48]");                                  // entry[48]: PREV slot index
    emitter.instruction("str x12, [sp, #16]");                                  // advance the cursor BEFORE any call clobbers it
    emitter.instruction("ldp x1, x2, [x8, #8]");                                // entry key pointer and length
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the entry's key pair
    emitter.instruction("ldp x3, x4, [x8, #24]");                               // entry value low and high words
    emitter.instruction("stp x3, x4, [sp, #40]");                               // save the entry's value pair
    emitter.instruction("ldr x5, [x8, #40]");                                   // entry value tag
    emitter.instruction("str x5, [sp, #56]");                                   // save the entry's value tag

    emitter.instruction("ldr x9, [sp, #64]");                                   // value_type
    emitter.instruction("cmp x9, #1");                                          // strings are copied, not shared
    emitter.instruction("b.eq __rt_h2h_rev_persist");                           // strings get an independent copy
    emitter.instruction("cmp x9, #4");                                          // below the heap-backed tag range?
    emitter.instruction("b.lt __rt_h2h_rev_insert");                            // scalars need no retain
    emitter.instruction("cmp x9, #7");                                          // above the heap-backed tag range?
    emitter.instruction("b.gt __rt_h2h_rev_insert");                            // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #40]");                                   // the heap-backed payload
    emitter.instruction("bl __rt_incref");                                      // retain it for the result
    emitter.instruction("b __rt_h2h_rev_insert");                               // payload retained: insert it
    emitter.label("__rt_h2h_rev_persist");
    emitter.instruction("ldr x1, [sp, #40]");                                   // string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // string length
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = an independent copy
    emitter.instruction("stp x1, x2, [sp, #40]");                               // the result stores the copy

    emitter.label("__rt_h2h_rev_insert");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the result hash
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // the entry's ORIGINAL key
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // the owned payload
    emitter.instruction("ldr x5, [sp, #56]");                                   // its runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert under the original key
    emitter.instruction("str x0, [sp, #8]");                                    // the result may have moved
    emitter.instruction("b __rt_h2h_rev_loop");                                 // process the previous entry

    emitter.label("__rt_h2h_rev_empty");
    emitter.instruction("mov x0, #8");                                          // a null source still answers an empty hash
    emitter.instruction("mov x1, #0");                                          // with no declared value type
    emitter.instruction("bl __rt_hash_new");                                     // allocate it
    emitter.instruction("str x0, [sp, #8]");                                    // and return it through the shared epilogue

    emitter.label("__rt_h2h_rev_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = the result hash
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the owned reversed hash to the caller
}

/// Emits `__rt_hash_to_hash_reverse` for x86_64.
///
/// Same frame contents as the ARM64 form, at `[rbp - 8]` … `[rbp - 72]`. Helper ABIs are spelled
/// out rather than deduced: `__rt_str_persist` reads its pointer from `rax` and its length from
/// `rdx`, while `__rt_hash_new`, `__rt_hash_set` and `__rt_incref` take SysV argument registers.
fn emit_hash_to_hash_reverse_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_to_hash_reverse ---");
    emitter.label_global("__rt_hash_to_hash_reverse");
    emitter.instruction("push rbp");                                            // save the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("sub rsp, 96");                                         // reserve the walk state, keeping rsp aligned

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source hash
    emitter.instruction("test rdi, rdi");                                       // a null table has nothing to reverse
    emitter.instruction("jz __rt_h2h_rev_empty_x");                             // answer an empty hash instead
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "r11",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp rdi, r11");                                        // does the table carry the in-band null-container sentinel?
    emitter.instruction("je __rt_h2h_rev_empty_x");                             // sentinel-null tables have nothing to reverse

    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // entry count, used as the capacity hint
    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // the hash header's value_type
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save it: payload ownership depends on it
    emitter.instruction("cmp rax, 8");                                          // below the minimum useful capacity?
    emitter.instruction("jge __rt_h2h_rev_cap_ok_x");                           // use the count as the hint
    emitter.instruction("mov rax, 8");                                          // clamp small sources to a floor
    emitter.label("__rt_h2h_rev_cap_ok_x");
    emitter.instruction("mov rdi, rax");                                        // capacity hint
    emitter.instruction("mov rsi, r10");                                        // the result carries the source's value_type
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the result hash

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source
    emitter.instruction("mov r11, QWORD PTR [rdi + 32]");                       // header[32]: TAIL slot index starts the reverse walk
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // park the cursor

    emitter.label("__rt_h2h_rev_loop_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // current slot index
    emitter.instruction("cmp r11, -1");                                         // has the walk run off the head?
    emitter.instruction("je __rt_h2h_rev_done_x");                              // walk finished: return the result
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // the source hash header
    super::hash_layout::emit_entry_address(emitter, "rax", "r10", "r11");
    emitter.instruction("mov r10, QWORD PTR [rax + 48]");                       // entry[48]: PREV slot index
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // advance the cursor BEFORE any call clobbers it
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // entry key pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save it
    emitter.instruction("mov r10, QWORD PTR [rax + 16]");                       // entry key length
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save it
    emitter.instruction("mov r10, QWORD PTR [rax + 24]");                       // entry value low word
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save it
    emitter.instruction("mov r10, QWORD PTR [rax + 32]");                       // entry value high word
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save it
    emitter.instruction("mov r10, QWORD PTR [rax + 40]");                       // entry value tag
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save it

    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // value_type
    emitter.instruction("cmp r10, 1");                                          // strings are copied, not shared
    emitter.instruction("je __rt_h2h_rev_persist_x");                           // strings get an independent copy
    emitter.instruction("cmp r10, 4");                                          // below the heap-backed tag range?
    emitter.instruction("jl __rt_h2h_rev_insert_x");                            // scalars need no retain
    emitter.instruction("cmp r10, 7");                                          // above the heap-backed tag range?
    emitter.instruction("jg __rt_h2h_rev_insert_x");                            // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // the heap-backed payload
    emitter.instruction("call __rt_incref");                                    // retain it for the result
    emitter.instruction("jmp __rt_h2h_rev_insert_x");                           // payload retained: insert it
    emitter.label("__rt_h2h_rev_persist_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // string length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = an independent copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // the result stores the copy
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // and the copy's length

    emitter.label("__rt_h2h_rev_insert_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the result hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // the entry's ORIGINAL key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // and its length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // the owned payload
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // its high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // its runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert under the original key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the result may have moved
    emitter.instruction("jmp __rt_h2h_rev_loop_x");                             // process the previous entry

    emitter.label("__rt_h2h_rev_empty_x");
    emitter.instruction("mov rdi, 8");                                          // a null source still answers an empty hash
    emitter.instruction("xor esi, esi");                                        // with no declared value type
    emitter.instruction("call __rt_hash_new");                                  // allocate it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // and return it through the shared epilogue

    emitter.label("__rt_h2h_rev_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = the result hash
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller's frame pointer
    emitter.instruction("ret");                                                 // return the owned reversed hash to the caller
}
