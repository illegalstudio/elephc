//! Purpose:
//! Emits the `__rt_hash_spread` runtime helper that flattens an associative
//! array into a destination hash using PHP `[...$source]` spread semantics.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - PHP spread reindexes integer-keyed source entries to fresh sequential keys
//!   (continuing from the destination's current max integer key + 1), preserves
//!   string keys, and lets later operands overwrite earlier ones on key collision.
//! - The destination hash takes its own owned copy of each value: strings are
//!   persisted, refcounted payloads are retained, and scalars are copied as-is.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_hash_spread` for the active target.
///
/// Iterates `source` in insertion order and inserts each entry into `dest`:
/// integer keys are replaced with the next automatic integer key (derived from
/// the destination's current largest integer key + 1, then incremented per
/// entry), and string keys are preserved. Each value is retained/persisted so
/// the destination owns an independent reference. Duplicate keys overwrite the
/// existing destination entry (later spread operand wins).
///
/// # Inputs (ARM64)
/// - `x0`: destination hash pointer
/// - `x1`: source hash pointer
///
/// # Outputs (ARM64)
/// - `x0`: possibly-reallocated destination hash pointer
///
/// Delegates to `emit_hash_spread_linux_x86_64` on x86_64.
pub fn emit_hash_spread(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_spread_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_spread ---");
    emitter.label_global("__rt_hash_spread");

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = destination hash pointer
    //   [sp, #8]  = source hash pointer
    //   [sp, #16] = running next automatic integer key
    //   [sp, #24] = insertion-order iterator cursor
    //   [sp, #32] = borrowed source key pointer
    //   [sp, #40] = borrowed source key length / integer sentinel
    //   [sp, #48] = borrowed source value low word
    //   [sp, #56] = borrowed source value high word
    //   [sp, #64] = borrowed source value runtime tag
    //   [sp, #80] = saved x29
    //   [sp, #88] = saved x30
    emitter.instruction("sub sp, sp, #96");                                     // reserve spill slots for the spread walk state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the destination hash pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the source hash pointer

    // -- derive the starting next integer key by scanning the destination for its largest integer key --
    emitter.instruction("ldr x5, [x0, #8]");                                    // load the destination hash capacity as the scan bound
    emitter.instruction("mov x6, #0");                                          // initialize the slot cursor
    emitter.instruction("mov x7, #0");                                          // track whether any integer key has been observed
    emitter.instruction("mov x8, #0");                                          // initialize the maximum integer key placeholder
    emitter.label("__rt_hash_spread_key_scan");
    emitter.instruction("cmp x6, x5");                                          // has every destination slot been inspected?
    emitter.instruction("b.ge __rt_hash_spread_key_scan_done");                 // finish scanning once the cursor reaches capacity
    emitter.instruction("mov x9, #64");                                         // x9 = hash entry size in bytes
    emitter.instruction("mul x10, x6, x9");                                     // convert the slot cursor into a byte offset
    emitter.instruction("add x10, x0, x10");                                    // advance from the hash base to the selected slot
    emitter.instruction("add x10, x10, #40");                                   // skip the fixed hash header to reach the entry fields
    emitter.instruction("ldr x11, [x10]");                                      // load the occupied marker for this slot
    emitter.instruction("cmp x11, #1");                                         // is this slot a live entry?
    emitter.instruction("b.ne __rt_hash_spread_key_scan_next");                 // ignore empty or tombstone slots while deriving the next key
    emitter.instruction("ldr x11, [x10, #16]");                                 // load the normalized key length or integer sentinel
    emitter.instruction("cmn x11, #1");                                         // check whether the key length is the integer-key sentinel
    emitter.instruction("b.ne __rt_hash_spread_key_scan_next");                 // string keys do not affect PHP's next automatic integer key
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored integer key payload
    emitter.instruction("cbz x7, __rt_hash_spread_key_scan_take");              // the first integer key seeds the maximum tracker
    emitter.instruction("cmp x12, x8");                                         // compare the candidate key against the current maximum
    emitter.instruction("b.le __rt_hash_spread_key_scan_next");                 // keep scanning if the candidate key is not larger
    emitter.label("__rt_hash_spread_key_scan_take");
    emitter.instruction("mov x8, x12");                                         // record the largest integer key seen so far
    emitter.instruction("mov x7, #1");                                          // remember that at least one integer key exists
    emitter.label("__rt_hash_spread_key_scan_next");
    emitter.instruction("add x6, x6, #1");                                      // advance to the next destination slot
    emitter.instruction("b __rt_hash_spread_key_scan");                         // continue scanning for integer keys
    emitter.label("__rt_hash_spread_key_scan_done");
    emitter.instruction("cbz x7, __rt_hash_spread_key_zero");                   // destinations with no integer keys start reindexing at zero
    emitter.instruction("add x8, x8, #1");                                      // start reindexing after the largest observed integer key
    emitter.instruction("b __rt_hash_spread_key_init");
    emitter.label("__rt_hash_spread_key_zero");
    emitter.instruction("mov x8, #0");                                          // first automatic integer key is zero
    emitter.label("__rt_hash_spread_key_init");
    emitter.instruction("str x8, [sp, #16]");                                   // save the running next integer key
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the insertion-order iterator cursor

    // -- walk source entries in insertion order and flatten them into the destination --
    emitter.label("__rt_hash_spread_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the source hash pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the insertion-order iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next source entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did the iterator report the terminal done sentinel?
    emitter.instruction("b.eq __rt_hash_spread_done");                          // finish once every source entry has been flattened
    emitter.instruction("str x0, [sp, #24]");                                   // save the next insertion-order iterator cursor
    emitter.instruction("str x1, [sp, #32]");                                   // save the borrowed source key pointer
    emitter.instruction("str x2, [sp, #40]");                                   // save the borrowed source key length / integer sentinel
    emitter.instruction("str x3, [sp, #48]");                                   // save the borrowed source value low word
    emitter.instruction("str x4, [sp, #56]");                                   // save the borrowed source value high word
    emitter.instruction("str x5, [sp, #64]");                                   // save the borrowed source value runtime tag

    // -- retain the source value so the destination owns an independent copy --
    emitter.instruction("ldr x5, [sp, #64]");                                   // reload the source value runtime tag
    emitter.instruction("cmp x5, #1");                                          // is the source value a string payload?
    emitter.instruction("b.eq __rt_hash_spread_value_str");                     // strings need a persisted copy for the destination owner
    emitter.instruction("cmp x5, #4");                                          // is the source value a refcounted indexed array?
    emitter.instruction("b.eq __rt_hash_spread_value_ref");                     // refcounted children need a reference bump
    emitter.instruction("cmp x5, #5");                                          // is the source value a refcounted associative array?
    emitter.instruction("b.eq __rt_hash_spread_value_ref");                     // refcounted children need a reference bump
    emitter.instruction("cmp x5, #6");                                          // is the source value a refcounted object?
    emitter.instruction("b.eq __rt_hash_spread_value_ref");                     // refcounted children need a reference bump
    emitter.instruction("cmp x5, #7");                                          // is the source value a boxed mixed cell?
    emitter.instruction("b.eq __rt_hash_spread_value_ref");                     // refcounted children need a reference bump
    emitter.instruction("ldr x3, [sp, #48]");                                   // reload the scalar value low word
    emitter.instruction("ldr x4, [sp, #56]");                                   // reload the scalar value high word
    emitter.instruction("ldr x5, [sp, #64]");                                   // reload the scalar value runtime tag
    emitter.instruction("b __rt_hash_spread_insert");                           // scalars are ready to insert

    emitter.label("__rt_hash_spread_value_str");
    emitter.instruction("ldr x1, [sp, #48]");                                   // load the borrowed source string pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // load the borrowed source string length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the destination owner
    emitter.instruction("str x1, [sp, #48]");                                   // save the owned string pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save the owned string length
    emitter.instruction("ldr x3, [sp, #48]");                                   // reload the owned string value low word
    emitter.instruction("ldr x4, [sp, #56]");                                   // reload the owned string value high word
    emitter.instruction("ldr x5, [sp, #64]");                                   // reload the string value runtime tag
    emitter.instruction("b __rt_hash_spread_insert");                           // insert the owned string payload

    emitter.label("__rt_hash_spread_value_ref");
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the borrowed refcounted child pointer
    emitter.instruction("bl __rt_incref");                                      // retain the child for the destination owner
    emitter.instruction("ldr x3, [sp, #48]");                                   // reload the retained child value low word
    emitter.instruction("mov x4, xzr");                                         // refcounted hash values store only the low payload word
    emitter.instruction("ldr x5, [sp, #64]");                                   // reload the refcounted value runtime tag

    // -- select the destination key: reindex integer keys, preserve string keys --
    emitter.label("__rt_hash_spread_insert");
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload the source key length / integer sentinel
    emitter.instruction("cmn x2, #1");                                          // is this an inline integer source key?
    emitter.instruction("b.eq __rt_hash_spread_int_key");                       // integer source keys are reindexed to the running counter
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the borrowed source string key pointer
    // x2 already holds the source string key length; __rt_hash_set persists it
    emitter.instruction("b __rt_hash_spread_set");                              // insert with the preserved string key

    emitter.label("__rt_hash_spread_int_key");
    emitter.instruction("ldr x1, [sp, #16]");                                   // load the running next integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks an integer key

    emitter.label("__rt_hash_spread_set");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination hash pointer
    // x3/x4/x5 already hold the retained value and runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert or overwrite the destination entry
    emitter.instruction("str x0, [sp, #0]");                                    // save the possibly reallocated destination hash pointer

    // -- advance the running integer key only when an integer source key was reindexed --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the source key length / integer sentinel
    emitter.instruction("cmn x9, #1");                                          // was the just-inserted source key an integer key?
    emitter.instruction("b.ne __rt_hash_spread_loop");                          // string keys do not advance the running integer counter
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the running next integer key
    emitter.instruction("add x9, x9, #1");                                      // advance the reindex counter past the inserted entry
    emitter.instruction("str x9, [sp, #16]");                                   // save the updated running integer key
    emitter.instruction("b __rt_hash_spread_loop");                             // continue flattening source entries

    // -- return the updated destination hash --
    emitter.label("__rt_hash_spread_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the updated destination hash pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the spread walk spill slots
    emitter.instruction("ret");                                                 // return to generated code
}

/// Emits `__rt_hash_spread` for the x86_64 Linux ABI.
///
/// Mirrors the ARM64 logic but uses System V AMD64 conventions:
/// - Inputs: `rdi` = destination hash, `rsi` = source hash
/// - Outputs: `rax` = updated destination hash
fn emit_hash_spread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_spread ---");
    emitter.label_global("__rt_hash_spread");

    // -- set up stack frame --
    // Frame layout:
    //   [rbp - 8]   = destination hash pointer
    //   [rbp - 16]  = source hash pointer
    //   [rbp - 24]  = running next automatic integer key
    //   [rbp - 32]  = insertion-order iterator cursor
    //   [rbp - 40]  = borrowed source key pointer
    //   [rbp - 48]  = borrowed source key length / integer sentinel
    //   [rbp - 56]  = borrowed source value low word
    //   [rbp - 64]  = borrowed source value high word
    //   [rbp - 72]  = borrowed source value runtime tag
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spread spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the spread walk state
    emitter.instruction("sub rsp, 80");                                         // reserve aligned spill space while keeping nested calls ABI-aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the destination hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the source hash pointer

    // -- derive the starting next integer key by scanning the destination for its largest integer key --
    emitter.instruction("mov r10, rdi");                                        // keep the destination hash pointer stable across the scan
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the destination hash capacity as the scan bound
    emitter.instruction("xor r8d, r8d");                                        // initialize the slot cursor
    emitter.instruction("xor r9d, r9d");                                        // track whether any integer key has been observed
    emitter.instruction("xor eax, eax");                                        // initialize the maximum integer key placeholder
    emitter.label("__rt_hash_spread_x86_key_scan");
    emitter.instruction("cmp r8, r11");                                         // has every destination slot been inspected?
    emitter.instruction("jge __rt_hash_spread_x86_key_scan_done");              // finish scanning once the cursor reaches capacity
    emitter.instruction("mov rcx, r8");                                         // copy the slot cursor before scaling it into a byte offset
    emitter.instruction("shl rcx, 6");                                          // convert the slot cursor into a 64-byte hash-entry offset
    emitter.instruction("add rcx, r10");                                        // advance from the hash base to the selected entry block
    emitter.instruction("add rcx, 40");                                         // skip the fixed hash header to reach the entry fields
    emitter.instruction("cmp QWORD PTR [rcx], 1");                              // is this slot a live entry?
    emitter.instruction("jne __rt_hash_spread_x86_key_scan_next");              // ignore empty or tombstone slots while deriving the next key
    emitter.instruction("cmp QWORD PTR [rcx + 16], -1");                        // is the normalized key an integer key?
    emitter.instruction("jne __rt_hash_spread_x86_key_scan_next");              // string keys do not affect PHP's next automatic integer key
    emitter.instruction("mov rdx, QWORD PTR [rcx + 8]");                        // load the stored integer key payload
    emitter.instruction("test r9, r9");                                         // has any integer key seeded the maximum tracker?
    emitter.instruction("je __rt_hash_spread_x86_key_scan_take");               // the first integer key becomes the current maximum
    emitter.instruction("cmp rdx, rax");                                        // compare the candidate key against the current maximum
    emitter.instruction("jle __rt_hash_spread_x86_key_scan_next");              // keep scanning if the candidate key is not larger
    emitter.label("__rt_hash_spread_x86_key_scan_take");
    emitter.instruction("mov rax, rdx");                                        // record the largest integer key seen so far
    emitter.instruction("mov r9, 1");                                           // remember that at least one integer key exists
    emitter.label("__rt_hash_spread_x86_key_scan_next");
    emitter.instruction("add r8, 1");                                           // advance to the next destination slot
    emitter.instruction("jmp __rt_hash_spread_x86_key_scan");                   // continue scanning for integer keys
    emitter.label("__rt_hash_spread_x86_key_scan_done");
    emitter.instruction("test r9, r9");                                         // did the scan observe any integer keys?
    emitter.instruction("je __rt_hash_spread_x86_key_zero");                    // destinations with no integer keys start reindexing at zero
    emitter.instruction("add rax, 1");                                          // start reindexing after the largest observed integer key
    emitter.instruction("jmp __rt_hash_spread_x86_key_init");
    emitter.label("__rt_hash_spread_x86_key_zero");
    emitter.instruction("xor eax, eax");                                        // first automatic integer key is zero
    emitter.label("__rt_hash_spread_x86_key_init");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the running next integer key
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the insertion-order iterator cursor

    emitter.label("__rt_hash_spread_x86_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the source hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the insertion-order iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next source entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did the iterator report the terminal done sentinel?
    emitter.instruction("je __rt_hash_spread_x86_done");                        // finish once every source entry has been flattened
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the next insertion-order iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the borrowed source key pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the borrowed source key length / integer sentinel
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the borrowed source value low word
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save the borrowed source value high word
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the borrowed source value runtime tag

    // -- retain the source value so the destination owns an independent copy --
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the source value runtime tag
    emitter.instruction("cmp r9, 1");                                           // is the source value a string payload?
    emitter.instruction("je __rt_hash_spread_x86_value_str");                   // strings need a persisted copy for the destination owner
    emitter.instruction("cmp r9, 4");                                           // is the source value a refcounted indexed array?
    emitter.instruction("je __rt_hash_spread_x86_value_ref");                   // refcounted children need a reference bump
    emitter.instruction("cmp r9, 5");                                           // is the source value a refcounted associative array?
    emitter.instruction("je __rt_hash_spread_x86_value_ref");                   // refcounted children need a reference bump
    emitter.instruction("cmp r9, 6");                                           // is the source value a refcounted object?
    emitter.instruction("je __rt_hash_spread_x86_value_ref");                   // refcounted children need a reference bump
    emitter.instruction("cmp r9, 7");                                           // is the source value a boxed mixed cell?
    emitter.instruction("je __rt_hash_spread_x86_value_ref");                   // refcounted children need a reference bump
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the scalar value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the scalar value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the scalar value runtime tag
    emitter.instruction("jmp __rt_hash_spread_x86_insert");                     // scalars are ready to insert

    emitter.label("__rt_hash_spread_x86_value_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the borrowed source string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // load the borrowed source string length
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload for the destination owner
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the owned string pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the owned string length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the owned string value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the owned string value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the string value runtime tag
    emitter.instruction("jmp __rt_hash_spread_x86_insert");                     // insert the owned string payload

    emitter.label("__rt_hash_spread_x86_value_ref");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the borrowed refcounted child pointer
    emitter.instruction("call __rt_incref");                                    // retain the child for the destination owner
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the retained child value low word
    emitter.instruction("xor r8d, r8d");                                        // refcounted hash values store only the low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the refcounted value runtime tag

    // -- select the destination key: reindex integer keys, preserve string keys --
    emitter.label("__rt_hash_spread_x86_insert");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the source key length / integer sentinel
    emitter.instruction("cmp rdx, -1");                                         // is this an inline integer source key?
    emitter.instruction("je __rt_hash_spread_x86_int_key");                     // integer source keys are reindexed to the running counter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the borrowed source string key pointer
    // rdx already holds the source string key length; __rt_hash_set persists it
    emitter.instruction("jmp __rt_hash_spread_x86_set");                        // insert with the preserved string key

    emitter.label("__rt_hash_spread_x86_int_key");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // load the running next integer key
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks an integer key

    emitter.label("__rt_hash_spread_x86_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the destination hash pointer
    // rcx/r8/r9 already hold the retained value and runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert or overwrite the destination entry
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the possibly reallocated destination hash pointer

    // -- advance the running integer key only when an integer source key was reindexed --
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the source key length / integer sentinel
    emitter.instruction("cmp r10, -1");                                         // was the just-inserted source key an integer key?
    emitter.instruction("jne __rt_hash_spread_x86_loop");                       // string keys do not advance the running integer counter
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the running next integer key
    emitter.instruction("add r10, 1");                                          // advance the reindex counter past the inserted entry
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the updated running integer key
    emitter.instruction("jmp __rt_hash_spread_x86_loop");                       // continue flattening source entries

    // -- return the updated destination hash --
    emitter.label("__rt_hash_spread_x86_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the updated destination hash pointer
    emitter.instruction("add rsp, 80");                                         // release the spread walk spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return to generated code
}