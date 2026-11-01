//! Purpose:
//! Emits `__rt_hash_reindex`, which renumbers a hash's INTEGER keys from zero while keeping its
//! string keys, in insertion order.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - This is php's rule for every `preserve_keys = false` container result: `array_reverse`,
//!   `array_slice` and `array_chunk` all renumber integer keys from zero in the NEW order and
//!   leave string keys alone. Without it a hash had no `false` arm at all — `array_reverse($h,
//!   false)` refused to compile (`unsupported EIR backend feature: array_reverse for PHP type
//!   AssocArray`), and a flag only known at run time could not be lowered for a hash either.
//! - Composes rather than duplicates: the caller runs the key-PRESERVING helper it already has and
//!   passes the result here, so one routine serves all three builtins.
//! - A key is an integer key when its length word is `-1`, which is the marker `__rt_hash_set`
//!   takes (`key_lo` is then the integer itself). The same encoding `__rt_array_to_hash_reverse`
//!   writes.
//! - Payload ownership mirrors `__rt_hash_to_hash_unique` and `__rt_hash_to_hash_reverse` exactly:
//!   strings are persisted into independent copies, heap-backed values (tags 4..7) are retained,
//!   scalars are copied as-is.
//! - The entry chain is followed directly (`head` at header[24], `next` at entry[56]) and the
//!   cursor is advanced BEFORE any call, because every per-entry helper clobbers the caller-saved
//!   registers the chain pointer would live in.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// hash_reindex: build an owned hash holding the source entries in their existing order, with
/// integer keys renumbered from zero and string keys untouched.
///
/// Input:  `x0` / `rdi` = source hash pointer
/// Output: `x0` / `rax` = new owned hash
pub fn emit_hash_reindex(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_reindex_x86_64(emitter);
        return;
    }
    emit_hash_reindex_aarch64(emitter);
}

/// Emits `__rt_hash_reindex` for ARM64.
///
/// Frame (112 bytes): `[0]` source, `[8]` result, `[16]` cursor slot index, `[24]` key pointer,
/// `[32]` key length, `[40]` value low, `[48]` value high, `[56]` value tag, `[64]` value_type,
/// `[72]` next integer key.
fn emit_hash_reindex_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_reindex ---");
    emitter.label_global("__rt_hash_reindex");
    emitter.instruction("sub sp, sp, #112");                                    // reserve the walk state
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the frame pointer

    emitter.instruction("str x0, [sp, #0]");                                    // save the source hash
    emitter.instruction("cbz x0, __rt_hash_reidx_empty");                       // a null table has nothing to renumber
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "x7",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp x0, x7");                                          // does the table carry the in-band null-container sentinel?
    emitter.instruction("b.eq __rt_hash_reidx_empty");                          // sentinel-null tables have nothing to renumber

    emitter.instruction("ldr x9, [x0]");                                        // entry count, used as the capacity hint
    emitter.instruction("ldr x10, [x0, #16]");                                  // the hash header's value_type
    emitter.instruction("str x10, [sp, #64]");                                  // save it: payload ownership depends on it
    emitter.instruction("cmp x9, #8");                                          // below the minimum useful capacity?
    emitter.instruction("b.ge __rt_hash_reidx_cap_ok");                         // use the count as the hint
    emitter.instruction("mov x9, #8");                                          // clamp small sources to a floor
    emitter.label("__rt_hash_reidx_cap_ok");
    emitter.instruction("mov x0, x9");                                          // capacity hint
    emitter.instruction("mov x1, x10");                                         // the result carries the source's value_type
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash

    emitter.instruction("mov x9, #0");                                          // the first renumbered integer key is zero
    emitter.instruction("str x9, [sp, #72]");                                   // park the running key counter
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source
    emitter.instruction("ldr x11, [x0, #24]");                                  // header[24]: HEAD slot index starts the forward walk
    emitter.instruction("str x11, [sp, #16]");                                  // park the cursor

    emitter.label("__rt_hash_reidx_loop");
    emitter.instruction("ldr x11, [sp, #16]");                                  // current slot index
    emitter.instruction("cmp x11, #-1");                                        // has the walk run off the tail?
    emitter.instruction("b.eq __rt_hash_reidx_done");                           // walk finished: return the result
    emitter.instruction("ldr x0, [sp, #0]");                                    // the source hash base
    super::hash_layout::emit_entry_address(emitter, "x8", "x0", "x11");
    emitter.instruction("ldr x12, [x8, #56]");                                  // entry[56]: NEXT slot index
    emitter.instruction("str x12, [sp, #16]");                                  // advance the cursor BEFORE any call clobbers it
    emitter.instruction("ldp x1, x2, [x8, #8]");                                // entry key pointer and length
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the entry's key pair
    emitter.instruction("ldp x3, x4, [x8, #24]");                               // entry value low and high words
    emitter.instruction("stp x3, x4, [sp, #40]");                               // save the entry's value pair
    emitter.instruction("ldr x5, [x8, #40]");                                   // entry value tag
    emitter.instruction("str x5, [sp, #56]");                                   // save the entry's value tag

    emitter.instruction("ldr x9, [sp, #64]");                                   // value_type
    emitter.instruction("cmp x9, #1");                                          // strings are copied, not shared
    emitter.instruction("b.eq __rt_hash_reidx_persist");                        // strings get an independent copy
    emitter.instruction("cmp x9, #4");                                          // below the heap-backed tag range?
    emitter.instruction("b.lt __rt_hash_reidx_key");                            // scalars need no retain
    emitter.instruction("cmp x9, #7");                                          // above the heap-backed tag range?
    emitter.instruction("b.gt __rt_hash_reidx_key");                            // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #40]");                                   // the heap-backed payload
    emitter.instruction("bl __rt_incref");                                      // retain it for the result
    emitter.instruction("b __rt_hash_reidx_key");                               // payload retained: choose the key
    emitter.label("__rt_hash_reidx_persist");
    emitter.instruction("ldr x1, [sp, #40]");                                   // string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // string length
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = an independent copy
    emitter.instruction("stp x1, x2, [sp, #40]");                               // the result stores the copy

    emitter.label("__rt_hash_reidx_key");
    emitter.instruction("ldr x2, [sp, #32]");                                   // the entry's key length word
    emitter.instruction("cmn x2, #1");                                          // length -1 is the integer-key marker
    emitter.instruction("b.ne __rt_hash_reidx_insert");                         // a string key keeps its own spelling
    emitter.instruction("ldr x9, [sp, #72]");                                   // the next renumbered integer key
    emitter.instruction("str x9, [sp, #24]");                                   // it replaces the original integer
    emitter.instruction("add x9, x9, #1");                                      // advance the counter for the next integer key
    emitter.instruction("str x9, [sp, #72]");                                   // park it again

    emitter.label("__rt_hash_reidx_insert");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the result hash
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // the key to insert under
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // the owned payload
    emitter.instruction("ldr x5, [sp, #56]");                                   // its runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry
    emitter.instruction("str x0, [sp, #8]");                                    // the result may have moved
    emitter.instruction("b __rt_hash_reidx_loop");                              // process the next entry

    emitter.label("__rt_hash_reidx_empty");
    emitter.instruction("mov x0, #8");                                          // a null source still answers an empty hash
    emitter.instruction("mov x1, #0");                                          // with no declared value type
    emitter.instruction("bl __rt_hash_new");                                    // allocate it
    emitter.instruction("str x0, [sp, #8]");                                    // and return it through the shared epilogue

    emitter.label("__rt_hash_reidx_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = the result hash
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the frame
    emitter.instruction("ret");                                                 // return the owned renumbered hash to the caller
}

/// Emits `__rt_hash_reindex` for x86_64.
///
/// Same frame contents as the ARM64 form, at `[rbp - 8]` … `[rbp - 80]`. Helper ABIs are spelled
/// out rather than deduced: `__rt_str_persist` reads its pointer from `rax` and its length from
/// `rdx`, while `__rt_hash_new`, `__rt_hash_set` and `__rt_incref` take SysV argument registers.
fn emit_hash_reindex_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_reindex ---");
    emitter.label_global("__rt_hash_reindex");
    emitter.instruction("push rbp");                                            // save the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("sub rsp, 96");                                         // reserve the walk state, keeping rsp aligned

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source hash
    emitter.instruction("test rdi, rdi");                                       // a null table has nothing to renumber
    emitter.instruction("jz __rt_hash_reidx_empty_x");                          // answer an empty hash instead
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "r11",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp rdi, r11");                                        // does the table carry the in-band null-container sentinel?
    emitter.instruction("je __rt_hash_reidx_empty_x");                          // sentinel-null tables have nothing to renumber

    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // entry count, used as the capacity hint
    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // the hash header's value_type
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save it: payload ownership depends on it
    emitter.instruction("cmp rax, 8");                                          // below the minimum useful capacity?
    emitter.instruction("jge __rt_hash_reidx_cap_ok_x");                        // use the count as the hint
    emitter.instruction("mov rax, 8");                                          // clamp small sources to a floor
    emitter.label("__rt_hash_reidx_cap_ok_x");
    emitter.instruction("mov rdi, rax");                                        // capacity hint
    emitter.instruction("mov rsi, r10");                                        // the result carries the source's value_type
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the result hash

    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // the first renumbered integer key is zero
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source
    emitter.instruction("mov r11, QWORD PTR [rdi + 24]");                       // header[24]: HEAD slot index starts the forward walk
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // park the cursor

    emitter.label("__rt_hash_reidx_loop_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // current slot index
    emitter.instruction("cmp r11, -1");                                         // has the walk run off the tail?
    emitter.instruction("je __rt_hash_reidx_done_x");                           // walk finished: return the result
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // the source hash header
    super::hash_layout::emit_entry_address(emitter, "rax", "r10", "r11");
    emitter.instruction("mov r10, QWORD PTR [rax + 56]");                       // entry[56]: NEXT slot index
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
    emitter.instruction("je __rt_hash_reidx_persist_x");                        // strings get an independent copy
    emitter.instruction("cmp r10, 4");                                          // below the heap-backed tag range?
    emitter.instruction("jl __rt_hash_reidx_key_x");                            // scalars need no retain
    emitter.instruction("cmp r10, 7");                                          // above the heap-backed tag range?
    emitter.instruction("jg __rt_hash_reidx_key_x");                            // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // the heap-backed payload
    emitter.instruction("call __rt_incref");                                    // retain it for the result
    emitter.instruction("jmp __rt_hash_reidx_key_x");                           // payload retained: choose the key
    emitter.label("__rt_hash_reidx_persist_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // string length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = an independent copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // the result stores the copy
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // and the copy's length

    emitter.label("__rt_hash_reidx_key_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // the entry's key length word
    emitter.instruction("cmp r10, -1");                                         // length -1 is the integer-key marker
    emitter.instruction("jne __rt_hash_reidx_insert_x");                        // a string key keeps its own spelling
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // the next renumbered integer key
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // it replaces the original integer
    emitter.instruction("add r10, 1");                                          // advance the counter for the next integer key
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // park it again

    emitter.label("__rt_hash_reidx_insert_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the result hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // the key to insert under
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // and its length word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // the owned payload
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // its high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // its runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert the entry
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the result may have moved
    emitter.instruction("jmp __rt_hash_reidx_loop_x");                          // process the next entry

    emitter.label("__rt_hash_reidx_empty_x");
    emitter.instruction("mov rdi, 8");                                          // a null source still answers an empty hash
    emitter.instruction("xor esi, esi");                                        // with no declared value type
    emitter.instruction("call __rt_hash_new");                                  // allocate it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // and return it through the shared epilogue

    emitter.label("__rt_hash_reidx_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = the result hash
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller's frame pointer
    emitter.instruction("ret");                                                 // return the owned renumbered hash to the caller
}
