//! Purpose:
//! Emits `__rt_hash_to_hash_unique`, backing `array_unique()` when its argument is already a hash.
//! Keeps each value's FIRST key, which is what PHP returns.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `array_unique($assoc)` used to refuse to compile at all
//!   (`unsupported EIR backend feature: array_unique for PHP type AssocArray`), which is how an
//!   ordinary PHP call became a build failure.
//! - Duplicates are found through a SECOND hash keyed by the VALUE, not by scanning what has
//!   already been emitted. Hash keys compare by value, so equal strings at different addresses
//!   collapse for free — the defect that had to be fixed by hand in the indexed-array scan — and
//!   the walk stays linear instead of quadratic.
//! - Only uniform `int` and `string` value types are handled. A boxed value's key would be its
//!   POINTER, so it would dedupe by identity and answer wrongly; the lowering refuses those
//!   before reaching here, exactly as it does for indexed arrays.
//! - Payload handling mirrors `__rt_array_to_hash_unique`: strings are persisted into independent
//!   copies and heap-backed values retained, so the result owns what it holds.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// hash_to_hash_unique: build an owned hash holding the first occurrence of each value, keyed by
/// that occurrence's ORIGINAL key.
///
/// Input:  `x0` / `rdi` = source hash pointer
/// Output: `x0` / `rax` = new owned hash
pub fn emit_hash_to_hash_unique(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_to_hash_unique_x86_64(emitter);
        return;
    }
    emit_hash_to_hash_unique_aarch64(emitter);
}

/// Emits `__rt_hash_to_hash_unique` for ARM64.
///
/// Frame (96 bytes): `[0]` source, `[8]` result, `[16]` seen, `[24]` cursor, `[32]` key pointer,
/// `[40]` key length, `[48]` value low, `[56]` value high, `[64]` value tag, `[72]` value_type.
/// Every loop register is reloaded from the frame after a call, because all of them are
/// caller-saved and three helpers are called per entry.
fn emit_hash_to_hash_unique_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_to_hash_unique ---");
    emitter.label_global("__rt_hash_to_hash_unique");
    emitter.instruction("sub sp, sp, #96");                                     // reserve the walk state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the frame pointer

    emitter.instruction("str x0, [sp, #0]");                                    // save the source hash
    emitter.instruction("ldr x9, [x0]");                                        // entry count, used as the capacity hint
    emitter.instruction("ldr x10, [x0, #16]");                                  // the hash header's value_type
    emitter.instruction("str x10, [sp, #72]");                                  // save it: the seen-key shape depends on it
    emitter.instruction("cmp x9, #8");                                          // below the minimum useful capacity?
    emitter.instruction("b.ge __rt_h2h_uniq_cap_ok");                           // use the count as the hint
    emitter.instruction("mov x9, #8");                                          // clamp small sources to a floor
    emitter.label("__rt_h2h_uniq_cap_ok");
    emitter.instruction("str x9, [sp, #24]");                                   // park the capacity hint across the two allocations
    emitter.instruction("mov x0, x9");                                          // capacity hint
    emitter.instruction("mov x1, x10");                                         // the result carries the source's value_type
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash

    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the capacity hint
    emitter.instruction("mov x1, #0");                                          // the seen table only ever stores a marker int
    emitter.instruction("bl __rt_hash_new");                                    // allocate the seen table
    emitter.instruction("str x0, [sp, #16]");                                   // save the seen table
    emitter.instruction("str xzr, [sp, #24]");                                  // cursor = 0 starts the insertion-order walk

    emitter.label("__rt_h2h_uniq_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // source hash
    emitter.instruction("ldr x1, [sp, #24]");                                   // current cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=cursor, x1/x2=key, x3/x4=value, x5=tag
    emitter.instruction("cmp x0, #-1");                                         // has the walk finished?
    emitter.instruction("b.eq __rt_h2h_uniq_done");                             // walk finished: return the result
    emitter.instruction("str x0, [sp, #24]");                                   // save the resumed cursor
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the entry's key pair
    emitter.instruction("stp x3, x4, [sp, #48]");                               // save the entry's value pair
    emitter.instruction("str x5, [sp, #64]");                                   // save the entry's value tag

    // The seen-key IS the value: an int key for an int value, the (pointer, length) pair for a
    // string. Hash keys compare by value, so equal strings collapse without a byte scan here.
    emitter.instruction("ldr x0, [sp, #16]");                                   // the seen table
    emitter.instruction("ldr x1, [sp, #48]");                                   // value low word
    emitter.instruction("ldr x9, [sp, #72]");                                   // value_type
    emitter.instruction("cmp x9, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_h2h_uniq_str_probe");                        // strings probe with (pointer, length)
    emitter.instruction("mov x2, #-1");                                         // integer keys carry the -1 sentinel
    emitter.instruction("b __rt_h2h_uniq_probe");                               // integer key is ready to probe
    emitter.label("__rt_h2h_uniq_str_probe");
    emitter.instruction("ldr x2, [sp, #56]");                                   // string keys carry the length
    emitter.label("__rt_h2h_uniq_probe");
    emitter.instruction("bl __rt_hash_get");                                    // x0 = 1 when this value was already kept
    emitter.instruction("cbnz x0, __rt_h2h_uniq_loop");                         // duplicate: PHP keeps only the FIRST occurrence

    // Mark the value as seen before inserting, so the marker survives a reallocation of the result.
    emitter.instruction("ldr x0, [sp, #16]");                                   // the seen table
    emitter.instruction("ldr x1, [sp, #48]");                                   // value low word
    emitter.instruction("ldr x9, [sp, #72]");                                   // value_type
    emitter.instruction("cmp x9, #1");                                          // string values key on their length too
    emitter.instruction("b.eq __rt_h2h_uniq_str_mark");                         // strings mark with (pointer, length)
    emitter.instruction("mov x2, #-1");                                         // integer-key sentinel
    emitter.instruction("b __rt_h2h_uniq_mark");                                // integer key is ready to mark
    emitter.label("__rt_h2h_uniq_str_mark");
    emitter.instruction("ldr x2, [sp, #56]");                                   // string key length
    emitter.label("__rt_h2h_uniq_mark");
    emitter.instruction("mov x3, #1");                                          // any non-null marker will do
    emitter.instruction("mov x4, #0");                                          // markers have no high word
    emitter.instruction("mov x5, #0");                                          // runtime tag 0 = int
    emitter.instruction("bl __rt_hash_set");                                    // record the value in the seen table
    emitter.instruction("str x0, [sp, #16]");                                   // the seen table may have moved

    // The result takes ownership of the payload, the way the indexed-array builder does.
    emitter.instruction("ldr x9, [sp, #72]");                                   // value_type
    emitter.instruction("cmp x9, #1");                                          // strings are copied, not shared
    emitter.instruction("b.eq __rt_h2h_uniq_persist");                          // strings get an independent copy
    emitter.instruction("cmp x9, #4");                                          // below the heap-backed tag range?
    emitter.instruction("b.lt __rt_h2h_uniq_insert");                           // scalars need no retain
    emitter.instruction("cmp x9, #7");                                          // above the heap-backed tag range?
    emitter.instruction("b.gt __rt_h2h_uniq_insert");                           // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #48]");                                   // the heap-backed payload
    emitter.instruction("bl __rt_incref");                                      // retain it for the result
    emitter.instruction("b __rt_h2h_uniq_insert");                              // payload retained: insert it
    emitter.label("__rt_h2h_uniq_persist");
    emitter.instruction("ldr x1, [sp, #48]");                                   // string pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // string length
    emitter.instruction("bl __rt_str_persist");                                 // x1 = an independent copy
    emitter.instruction("stp x1, x2, [sp, #48]");                               // the result stores the copy

    emitter.label("__rt_h2h_uniq_insert");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the result hash
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // the entry's ORIGINAL key
    emitter.instruction("ldp x3, x4, [sp, #48]");                               // the owned payload
    emitter.instruction("ldr x5, [sp, #64]");                                   // its runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert under the original key
    emitter.instruction("str x0, [sp, #8]");                                    // the result may have moved
    emitter.instruction("b __rt_h2h_uniq_loop");                                // process the next entry

    emitter.label("__rt_h2h_uniq_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the seen table is scratch
    emitter.instruction("bl __rt_decref_hash");                                 // release it before returning
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = the result hash
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the frame
    emitter.instruction("ret");                                                 // return the owned unique hash to the caller
}

/// Emits `__rt_hash_to_hash_unique` for x86_64.
///
/// Same frame contents as the ARM64 form, at `[rbp - 8]` … `[rbp - 80]`. The helper ABIs differ
/// per target and are spelled out rather than mirrored: `__rt_hash_iter_next` takes SysV argument
/// registers and returns its tuple in `rax`/`rdi`/`rsi`/`rdx`/`rcx`/`r8`, while `__rt_decref_hash`
/// reads its pointer from `rax`.
fn emit_hash_to_hash_unique_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_to_hash_unique ---");
    emitter.label_global("__rt_hash_to_hash_unique");
    emitter.instruction("push rbp");                                            // save the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("sub rsp, 96");                                         // reserve the walk state, keeping rsp aligned

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source hash
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // entry count, used as the capacity hint
    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // the hash header's value_type
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // save it: the seen-key shape depends on it
    emitter.instruction("cmp rax, 8");                                          // below the minimum useful capacity?
    emitter.instruction("jge __rt_h2h_uniq_cap_ok_x");                          // use the count as the hint
    emitter.instruction("mov rax, 8");                                          // clamp small sources to a floor
    emitter.label("__rt_h2h_uniq_cap_ok_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // park the capacity hint
    emitter.instruction("mov rdi, rax");                                        // capacity hint
    emitter.instruction("mov rsi, r10");                                        // the result carries the source's value_type
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the result hash

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the capacity hint
    emitter.instruction("xor esi, esi");                                        // the seen table only ever stores a marker int
    emitter.instruction("call __rt_hash_new");                                  // allocate the seen table
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the seen table
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // cursor = 0 starts the walk

    emitter.label("__rt_h2h_uniq_loop_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // source hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // current cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rax=cursor, rdi/rdx=key, rcx/r8=value, r9=tag
    emitter.instruction("cmp rax, -1");                                         // has the walk finished?
    emitter.instruction("je __rt_h2h_uniq_done_x");                             // walk finished: return the result
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the resumed cursor
    // Read off the helper, not deduced from SysV: `__rt_hash_iter_next` returns its tuple in
    // rax/rdi/rdx/rcx/r8/r9, NOT in argument order. Deducing it saved four wrong registers.
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // key pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // key length
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // value low word
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // value high word
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // value tag

    // See the ARM64 path: the seen-key IS the value, and hash keys compare by value.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the seen table
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // value low word
    emitter.instruction("cmp QWORD PTR [rbp - 80], 1");                         // is the value a string?
    emitter.instruction("je __rt_h2h_uniq_str_probe_x");                        // strings probe with (pointer, length)
    emitter.instruction("mov rdx, -1");                                         // integer keys carry the -1 sentinel
    emitter.instruction("jmp __rt_h2h_uniq_probe_x");                           // integer key is ready to probe
    emitter.label("__rt_h2h_uniq_str_probe_x");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // string keys carry the length
    emitter.label("__rt_h2h_uniq_probe_x");
    emitter.instruction("call __rt_hash_get");                                  // rax = 1 when already kept
    emitter.instruction("test rax, rax");                                       // was this value already kept?
    emitter.instruction("jnz __rt_h2h_uniq_loop_x");                            // duplicate: keep only the FIRST occurrence

    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the seen table
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // value low word
    emitter.instruction("cmp QWORD PTR [rbp - 80], 1");                         // string values key on their length too
    emitter.instruction("je __rt_h2h_uniq_str_mark_x");                         // strings mark with (pointer, length)
    emitter.instruction("mov rdx, -1");                                         // integer-key sentinel
    emitter.instruction("jmp __rt_h2h_uniq_mark_x");                            // integer key is ready to mark
    emitter.label("__rt_h2h_uniq_str_mark_x");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // string key length
    emitter.label("__rt_h2h_uniq_mark_x");
    emitter.instruction("mov rcx, 1");                                          // any non-null marker will do
    emitter.instruction("xor r8d, r8d");                                        // markers have no high word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 = int
    emitter.instruction("call __rt_hash_set");                                  // record the value in the seen table
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the seen table may have moved

    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // value_type
    emitter.instruction("cmp r10, 1");                                          // strings are copied, not shared
    emitter.instruction("je __rt_h2h_uniq_persist_x");                          // strings get an independent copy
    emitter.instruction("cmp r10, 4");                                          // below the heap-backed tag range?
    emitter.instruction("jl __rt_h2h_uniq_insert_x");                           // scalars need no retain
    emitter.instruction("cmp r10, 7");                                          // above the heap-backed tag range?
    emitter.instruction("jg __rt_h2h_uniq_insert_x");                           // non-heap tags need no retain
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // the heap-backed payload — __rt_incref reads RAX, never rdi
    emitter.instruction("call __rt_incref");                                    // retain it for the result
    emitter.instruction("jmp __rt_h2h_uniq_insert_x");                          // payload retained: insert it
    emitter.label("__rt_h2h_uniq_persist_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // string length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = an independent copy
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // the result stores the copy
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // and the copy's length

    emitter.label("__rt_h2h_uniq_insert_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the result hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // the entry's ORIGINAL key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // and its length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // the owned payload
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // its high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // its runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert under the original key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the result may have moved
    emitter.instruction("jmp __rt_h2h_uniq_loop_x");                            // process the next entry

    emitter.label("__rt_h2h_uniq_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the seen table is scratch
    emitter.instruction("call __rt_decref_hash");                               // release it (this helper reads rax)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = the result hash
    emitter.instruction("mov rsp, rbp");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller's frame pointer
    emitter.instruction("ret");                                                 // return the owned unique hash to the caller
}
