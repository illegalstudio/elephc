//! Purpose:
//! Emits the `__rt_hash_slice` runtime helper assembly: `array_slice()` over an ASSOCIATIVE
//! source (a hash), producing a hash that holds one window of the source's entries.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The indexed counterparts live in `array_slice.rs` (renumbered result) and
//!   `array_slice_to_hash.rs` (`preserve_keys` result); this module is the hash-source path and
//!   covers BOTH `preserve_keys` modes through a runtime flag, because the two differ by one
//!   decision per entry and nothing else.
//! - `$offset`/`$length` count POSITIONS in insertion order, not keys, so the window is decided
//!   by walking the source with `__rt_hash_iter_next` and counting. The normalization itself is
//!   the shared `emit_slice_bounds` prologue, which reads the source length from the first header
//!   word — a hash stores its entry count there, exactly as an indexed array stores its length.
//! - `preserve_keys` is NOT "keep all keys" versus "drop all keys". php-src only ever renumbers
//!   INTEGER keys; a string key survives either way. The `false` mode therefore re-keys integer
//!   entries from a fresh counter and copies string entries verbatim.
//! - OWNERSHIP mirrors `__rt_hash_clone_shallow`, because the destination holds the same values:
//!   string keys and refcounted values are retained, string values are re-persisted, scalars are
//!   copied inline. A renumbered integer key needs no ownership at all.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slice_bounds::emit_slice_bounds;

/// Emits the `__rt_hash_slice` runtime helper for associative (hash) sources.
///
/// # ABI
/// - Input: `x0` / `rdi` = source hash pointer, `x1` / `rsi` = `$offset`, `x2` / `rdx` = `$length`,
///   `x3` / `rcx` = whether `$length` was passed at all (0 = omitted, take everything from the
///   offset), `x4` / `r8` = `preserve_keys` (0 = renumber integer keys, 1 = keep every key).
/// - Output: `x0` / `rax` = destination hash pointer.
///
/// Dispatches to the target-specific implementation; x86_64 uses the System V register
/// convention, every other target uses the AArch64 path.
pub fn emit_hash_slice(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_slice_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_slice ---");
    emitter.label_global("__rt_hash_slice");

    // -- set up the frame and preserve the callee-saved registers --
    // Stack layout:
    //   [sp, #0]   = insertion-order iterator cursor
    //   [sp, #8]   = source key pointer (or inline integer key payload)
    //   [sp, #16]  = source key length (-1 marks an inline integer key)
    //   [sp, #24]  = source value_lo
    //   [sp, #32]  = source value_hi
    //   [sp, #40]  = source value_tag
    //   [sp, #48]  = window start (normalized offset)
    //   [sp, #56]  = window end (offset + normalized length)
    //   [sp, #64]  = next integer key for the renumbering mode
    //   [sp, #80]  = saved x19/x20
    //   [sp, #96]  = saved x21/x22
    //   [sp, #112] = saved x29/x30
    emitter.instruction("sub sp, sp, #128");                                    // allocate the hash-slice frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // set up the hash-slice frame pointer
    emitter.instruction("stp x19, x20, [sp, #80]");                             // save callee-saved x19/x20 for the source and destination tables
    emitter.instruction("stp x21, x22, [sp, #96]");                             // save callee-saved x21/x22 for the preserve flag and the position

    // -- take the mode flag out of the argument registers before the window prologue runs --
    // It survives `emit_slice_bounds` today, whose documented clobbers are x9/x10 only, but the
    // x86_64 side spills its flag first and this one should not be the one that has to re-read
    // that list when the prologue changes (issue #1093).
    emitter.instruction("mov x21, x4");                                         // x21 = preserve_keys flag, live across every helper call

    // -- normalize the PHP window before anything else clobbers the argument registers --
    // The prologue reads the source length from [x0], which for a hash is the entry count, and
    // leaves x1 = start position and x2 = element count.
    emit_slice_bounds(emitter, "__rt_hash_slice");
    emitter.instruction("mov x19, x0");                                         // x19 = source hash pointer, live across every helper call
    emitter.instruction("str x1, [sp, #48]");                                   // save the window start position
    emitter.instruction("add x9, x1, x2");                                      // x9 = one past the last position the window takes
    emitter.instruction("str x9, [sp, #56]");                                   // save the window end position

    // -- allocate the destination table sized for the window, never below the runtime minimum --
    emitter.instruction("lsl x0, x2, #1");                                      // double the window size to give the destination insertion headroom
    emitter.instruction("mov x9, #16");                                         // x9 = minimum destination bucket count
    emitter.instruction("cmp x0, x9");                                          // compare the derived capacity against the runtime minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp small windows up to the minimum bucket count
    emitter.instruction("ldr x1, [x19, #16]");                                  // x1 = source runtime value_type tag; the window holds the same values
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash table
    emitter.instruction("mov x20, x0");                                         // x20 = destination hash pointer, updated after every insertion
    emitter.instruction("str xzr, [sp, #0]");                                   // iterator cursor = 0 (start from header.head)
    emitter.instruction("str xzr, [sp, #64]");                                  // the renumbering counter starts at key 0, as php-src does
    emitter.instruction("mov x22, xzr");                                        // x22 = current source position, counted in insertion order

    // -- walk the source hash in insertion order, copying only the window --
    emitter.label("__rt_hash_slice_loop");
    emitter.instruction("mov x0, x19");                                         // x0 = source hash pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next source entry
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_slice_done");                           // yes - the destination hash is complete
    emitter.instruction("str x0, [sp, #0]");                                    // save the next insertion-order cursor
    emitter.instruction("str x1, [sp, #8]");                                    // save the source key pointer before helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save the source key length before helper calls
    emitter.instruction("str x3, [sp, #24]");                                   // save the source value_lo before helper calls
    emitter.instruction("str x4, [sp, #32]");                                   // save the source value_hi before helper calls
    emitter.instruction("str x5, [sp, #40]");                                   // save the source value_tag before helper calls

    // -- decide whether this position belongs to the window --
    emitter.instruction("ldr x9, [sp, #48]");                                   // x9 = window start position
    emitter.instruction("cmp x22, x9");                                         // is this entry still before the window?
    emitter.instruction("b.lt __rt_hash_slice_advance");                        // skip entries ahead of the offset
    emitter.instruction("ldr x9, [sp, #56]");                                   // x9 = window end position
    emitter.instruction("cmp x22, x9");                                         // has the walk passed the end of the window?
    emitter.instruction("b.ge __rt_hash_slice_done");                           // every later entry is outside it too, so stop walking

    // -- copy the key: verbatim when preserving, renumbered when the source key is an integer --
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = source key length
    emitter.instruction("cmn x2, #1");                                          // does this entry store an inline integer key?
    emitter.instruction("b.ne __rt_hash_slice_key_str");                        // string keys survive both modes unchanged
    emitter.instruction("cbnz x21, __rt_hash_slice_key_ready");                 // preserve_keys keeps the original integer key
    emitter.instruction("ldr x9, [sp, #64]");                                   // x9 = next renumbered integer key
    emitter.instruction("str x9, [sp, #8]");                                    // the entry takes the fresh key instead of its own
    emitter.instruction("add x9, x9, #1");                                      // advance the renumbering counter for the next integer entry
    emitter.instruction("str x9, [sp, #64]");                                   // save the advanced renumbering counter
    emitter.instruction("b __rt_hash_slice_key_ready");                         // the renumbered integer key needs no ownership

    emitter.label("__rt_hash_slice_key_str");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = source string key pointer for incref
    emitter.instruction("bl __rt_incref");                                      // retain the shared key for the destination table
    emitter.label("__rt_hash_slice_key_ready");

    // -- duplicate or retain the entry value according to this entry's runtime tag --
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = source entry value_tag
    emitter.instruction("cmp x5, #1");                                          // is this entry's value a string?
    emitter.instruction("b.eq __rt_hash_slice_value_str");                      // string values need fresh persisted payloads
    emitter.instruction("cmp x5, #4");                                          // is this entry's value an indexed array?
    emitter.instruction("b.eq __rt_hash_slice_value_ref");                      // nested refcounted values need retains
    emitter.instruction("cmp x5, #5");                                          // is this entry's value an associative array?
    emitter.instruction("b.eq __rt_hash_slice_value_ref");                      // nested refcounted values need retains
    emitter.instruction("cmp x5, #6");                                          // is this entry's value an object?
    emitter.instruction("b.eq __rt_hash_slice_value_ref");                      // nested refcounted values need retains
    emitter.instruction("cmp x5, #7");                                          // is this entry's value a boxed mixed cell?
    emitter.instruction("b.eq __rt_hash_slice_value_ref");                      // nested refcounted values need retains
    emitter.instruction("cmp x5, #10");                                         // is this entry's value a callable descriptor?
    emitter.instruction("b.eq __rt_hash_slice_value_ref");                      // runtime descriptors need retains; static descriptors are ignored by incref
    emitter.instruction("ldr x3, [sp, #24]");                                   // x3 = scalar/float value_lo copied as-is
    emitter.instruction("ldr x4, [sp, #32]");                                   // x4 = scalar/float value_hi copied as-is
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = scalar/float/null value_tag copied as-is
    emitter.instruction("b __rt_hash_slice_insert");                            // scalars are ready to insert immediately

    emitter.label("__rt_hash_slice_value_str");
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = source string value pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // x2 = source string value length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string value for the destination table
    emitter.instruction("mov x3, x1");                                          // x3 = owned string value pointer
    emitter.instruction("mov x4, x2");                                          // x4 = owned string value length
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = string value_tag copied as-is
    emitter.instruction("b __rt_hash_slice_insert");                            // insert the copied string value

    emitter.label("__rt_hash_slice_value_ref");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = source refcounted child pointer
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the destination table
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the retained child pointer after the helper call
    emitter.instruction("mov x4, xzr");                                         // refcounted hash values store only value_lo
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = refcounted value_tag copied as-is

    // -- insert the fully owned entry into the destination table --
    emitter.label("__rt_hash_slice_insert");
    emitter.instruction("mov x0, x20");                                         // x0 = destination hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = owned key pointer or inline integer key
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = owned key length (-1 for an integer key)
    emitter.instruction("bl __rt_hash_insert_owned");                           // insert the owned key/value pair into the destination table
    emitter.instruction("mov x20, x0");                                         // keep the destination pointer current after possible growth

    emitter.label("__rt_hash_slice_advance");
    emitter.instruction("add x22, x22, #1");                                    // this source position has been decided; move to the next
    emitter.instruction("b __rt_hash_slice_loop");                              // continue walking the source hash

    emitter.label("__rt_hash_slice_done");
    emitter.instruction("mov x0, x20");                                         // return the destination hash pointer
    emitter.instruction("ldp x21, x22, [sp, #96]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #80]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate the hash-slice frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash pointer
}

/// Emits the x86_64 Linux variant of the `__rt_hash_slice` runtime helper.
///
/// Mirrors the AArch64 logic step for step; only the register names and the branch-based clamps
/// differ. See [`emit_hash_slice`] for the full ABI and semantics.
///
/// The runtime's own helpers do not all follow System V here: `__rt_hash_iter_next` returns its
/// entry tuple across `rax`/`rdi`/`rdx`/`rcx`/`r8`/`r9`, `__rt_incref` takes its pointer in `rax`,
/// and `__rt_str_persist` takes and returns `rax`/`rdx`. This mirrors `__rt_hash_clone_shallow`,
/// which is the same walk over the same helpers.
fn emit_hash_slice_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_slice ---");
    emitter.label_global("__rt_hash_slice");

    // -- set up the frame and preserve the callee-saved registers --
    // Frame layout (rbp-relative, mirroring the AArch64 slots):
    //   [rbp - 8]   = destination hash pointer (also the return value)
    //   [rbp - 16]  = insertion-order iterator cursor
    //   [rbp - 24]  = source key pointer (or inline integer key payload)
    //   [rbp - 32]  = source key length (-1 marks an inline integer key)
    //   [rbp - 40]  = source value_lo
    //   [rbp - 48]  = source value_hi
    //   [rbp - 56]  = source value_tag
    //   [rbp - 64]  = window start (normalized offset)
    //   [rbp - 72]  = window end (offset + normalized length)
    //   [rbp - 80]  = next integer key for the renumbering mode
    //   [rbp - 88]  = preserve_keys flag
    //   [rbp - 96]  = current source position, counted in insertion order
    //   [rbp - 104] = saved r12, [rbp - 112] = saved r13
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the slice-state spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the window bounds and iterator state
    emitter.instruction("sub rsp, 128");                                        // reserve aligned spill space plus the callee-saved register slots
    emitter.instruction("mov QWORD PTR [rbp - 104], r12");                      // preserve r12 because the slice walk uses it as the long-lived source hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 112], r13");                      // preserve r13 because the slice walk uses it as the long-lived destination hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save the preserve_keys flag before any helper call can clobber it

    // -- normalize the PHP window before anything else clobbers the argument registers --
    // The prologue reads the source length from [rdi], which for a hash is the entry count, and
    // leaves rsi = start position and rdx = element count.
    emit_slice_bounds(emitter, "__rt_hash_slice");
    emitter.instruction("mov r12, rdi");                                        // keep the source hash pointer in a callee-saved register across the whole walk
    emitter.instruction("mov QWORD PTR [rbp - 64], rsi");                       // save the window start position
    emitter.instruction("lea rax, [rsi + rdx]");                                // rax = one past the last position the window takes
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the window end position

    // -- allocate the destination table sized for the window, never below the runtime minimum --
    emitter.instruction("lea rdi, [rdx + rdx]");                                // double the window size to give the destination insertion headroom
    emitter.instruction("cmp rdi, 16");                                         // compare the derived capacity against the runtime minimum
    emitter.instruction("jge __rt_hash_slice_cap_ready_x86");                   // keep a capacity that already clears the minimum
    emitter.instruction("mov rdi, 16");                                         // clamp small windows up to the minimum bucket count
    emitter.label("__rt_hash_slice_cap_ready_x86");
    emitter.instruction("mov rsi, QWORD PTR [r12 + 16]");                       // rsi = source runtime value_type tag; the window holds the same values
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table
    emitter.instruction("mov r13, rax");                                        // keep the destination hash pointer in a callee-saved register across the walk
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the destination pointer in the spill area for the return path
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // iterator cursor = 0 (start from header.head)
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // the renumbering counter starts at key 0, as php-src does
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // the walk starts at source position 0

    // -- walk the source hash in insertion order, copying only the window --
    emitter.label("__rt_hash_slice_loop");
    emitter.instruction("mov rdi, r12");                                        // pass the source hash pointer to the insertion-order iterator helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the saved cursor so iteration resumes from the previous returned slot
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next source entry with its key/value payload tuple
    emitter.instruction("cmp rax, -1");                                         // did the iterator report that no more entries remain?
    emitter.instruction("je __rt_hash_slice_done");                             // the destination hash is complete
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the source key pointer across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the source key length across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the source value_lo across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // preserve the source value_hi across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // preserve the source runtime value_tag across nested helper calls

    // -- decide whether this position belongs to the window --
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // r10 = current source position
    emitter.instruction("cmp r10, QWORD PTR [rbp - 64]");                       // is this entry still before the window?
    emitter.instruction("jl __rt_hash_slice_advance");                          // skip entries ahead of the offset
    emitter.instruction("cmp r10, QWORD PTR [rbp - 72]");                       // has the walk passed the end of the window?
    emitter.instruction("jge __rt_hash_slice_done");                            // every later entry is outside it too, so stop walking

    // -- copy the key: verbatim when preserving, renumbered when the source key is an integer --
    emitter.instruction("cmp QWORD PTR [rbp - 32], -1");                        // does this entry store an inline integer key?
    emitter.instruction("jne __rt_hash_slice_key_str");                         // string keys survive both modes unchanged
    emitter.instruction("cmp QWORD PTR [rbp - 88], 0");                         // is preserve_keys set?
    emitter.instruction("jne __rt_hash_slice_key_ready");                       // preserve_keys keeps the original integer key
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // rax = next renumbered integer key
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the entry takes the fresh key instead of its own
    emitter.instruction("add rax, 1");                                          // advance the renumbering counter for the next integer entry
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the advanced renumbering counter
    emitter.instruction("jmp __rt_hash_slice_key_ready");                       // the renumbered integer key needs no ownership

    emitter.label("__rt_hash_slice_key_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the shared source key pointer into the incref helper's input register
    emitter.instruction("call __rt_incref");                                    // retain the shared key for the destination table
    emitter.label("__rt_hash_slice_key_ready");

    // -- duplicate or retain the entry value according to this entry's runtime tag --
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // r10 = source entry value_tag
    emitter.instruction("cmp r10, 1");                                          // is this entry's value a string?
    emitter.instruction("je __rt_hash_slice_value_str");                        // string values need fresh persisted payloads
    emitter.instruction("cmp r10, 4");                                          // is this entry's value an indexed array?
    emitter.instruction("je __rt_hash_slice_value_ref");                        // nested refcounted values need retains
    emitter.instruction("cmp r10, 5");                                          // is this entry's value an associative array?
    emitter.instruction("je __rt_hash_slice_value_ref");                        // nested refcounted values need retains
    emitter.instruction("cmp r10, 6");                                          // is this entry's value an object?
    emitter.instruction("je __rt_hash_slice_value_ref");                        // nested refcounted values need retains
    emitter.instruction("cmp r10, 7");                                          // is this entry's value a boxed mixed cell?
    emitter.instruction("je __rt_hash_slice_value_ref");                        // nested refcounted values need retains
    emitter.instruction("cmp r10, 10");                                         // is this entry's value a callable descriptor?
    emitter.instruction("je __rt_hash_slice_value_ref");                        // runtime descriptors need retains; static descriptors are ignored by incref
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // rcx = scalar/float value_lo copied as-is
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // r8 = scalar/float value_hi copied as-is
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // r9 = scalar/float/null value_tag copied as-is
    emitter.instruction("jmp __rt_hash_slice_insert");                          // scalars are ready to insert immediately

    emitter.label("__rt_hash_slice_value_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // move the source string value pointer into the string helper's input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // move the source string value length into the paired helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the string value so the destination owns independent storage
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the copied string pointer for the destination insert
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the copied string length for the destination insert
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // rcx = owned string value pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // r8 = owned string value length
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // r9 = string value_tag copied as-is
    emitter.instruction("jmp __rt_hash_slice_insert");                          // insert the copied string value

    emitter.label("__rt_hash_slice_value_ref");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // load the shared refcounted child pointer into the incref helper's input register
    emitter.instruction("call __rt_incref");                                    // retain the shared child pointer for the destination table
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // rcx = retained child pointer
    emitter.instruction("xor r8d, r8d");                                        // refcounted hash values store only value_lo
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // r9 = refcounted value_tag copied as-is

    // -- insert the fully owned entry into the destination table --
    emitter.label("__rt_hash_slice_insert");
    emitter.instruction("mov rdi, r13");                                        // rdi = destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // rsi = owned key pointer or inline integer key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // rdx = owned key length (-1 for an integer key)
    emitter.instruction("call __rt_hash_insert_owned");                         // insert the owned key/value pair into the destination table
    emitter.instruction("mov r13, rax");                                        // keep the destination pointer current after possible growth
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the latest destination pointer for the return path

    emitter.label("__rt_hash_slice_advance");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // rax = current source position
    emitter.instruction("add rax, 1");                                          // this source position has been decided; move to the next
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save the advanced source position
    emitter.instruction("jmp __rt_hash_slice_loop");                            // continue walking the source hash

    emitter.label("__rt_hash_slice_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the destination hash pointer
    emitter.instruction("mov r13, QWORD PTR [rbp - 112]");                      // restore callee-saved r13
    emitter.instruction("mov r12, QWORD PTR [rbp - 104]");                      // restore callee-saved r12
    emitter.instruction("add rsp, 128");                                        // release the slice-state spill area
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = destination hash pointer
}
