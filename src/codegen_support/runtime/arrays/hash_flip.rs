//! Purpose:
//! Emits the `__rt_hash_flip` runtime helper assembly: `array_flip()` over an ASSOCIATIVE
//! source (a hash), producing a hash whose keys come from the source values and whose values
//! come from the source keys.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The indexed-array counterparts live in `array_flip.rs` / `array_flip_string.rs`; this
//!   module is the hash-source path and dispatches on the per-entry RUNTIME value tag, so one
//!   helper covers `AssocArray` with `Int`, `Str`, and boxed `Mixed` values.
//! - php-src `array_flip()` warns and SKIPS any entry whose value is not int or string. The
//!   `skip` arm reproduces that: it emits `ARRAY_FLIP_SKIPPED_MESSAGES` through
//!   `__rt_diag_warning` and moves to the next entry instead of inserting.
//! - OWNERSHIP: the source hash is only ever READ; this helper never retains or releases a
//!   source key or value. The destination owns fresh copies of both — `__rt_hash_set` persists
//!   the inserted string KEY itself, and the flipped VALUE (the source key) is copied through
//!   `__rt_str_persist` before insertion. No refcount traffic means no double-free surface.
//! - String values become keys through `__rt_hash_normalize_key`, so PHP numeric-string values
//!   collapse to integer keys exactly as php-src does (`array_flip(["a" => "5"]) === [5 => "a"]`).

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The `array_flip()` warning text for a value PHP refuses to use as an array key.
///
/// Entries are `(symbol, message)`. `crate::codegen_support::runtime::data::fixed` emits them
/// verbatim as `.ascii` literals; this module derives the `write()` length from `message.len()`
/// so the bytes and the immediate can never drift apart.
///
/// Captured from PHP 8.5.6 with `php -d xdebug.mode=off`; elephc does not synthesize the
/// ` in <file> on line <n>` tail that php-src appends to the message.
pub const ARRAY_FLIP_SKIPPED_MESSAGES: &[(&str, &str)] = &[(
    "_diag_array_flip_skipped",
    "Warning: array_flip(): Can only flip string and integer values, entry skipped\n",
)];

/// Returns the byte length of the single `array_flip()` skip message.
fn skip_message_len() -> usize {
    ARRAY_FLIP_SKIPPED_MESSAGES[0].1.len()
}

/// Emits the `__rt_hash_flip` runtime helper for associative (hash) sources.
///
/// Walks the source hash in insertion order and inserts one flipped entry per source entry:
/// the source VALUE becomes the destination key and the source KEY becomes the destination
/// value. Values that are neither int nor string are skipped with PHP's `E_WARNING`.
///
/// # ABI
/// - Input: `x0` / `rdi` = source hash pointer, `x1` / `rsi` = destination `value_type` tag
///   (the runtime tag of the SOURCE key type, since source keys become destination values).
/// - Output: `x0` / `rax` = destination hash pointer.
///
/// Dispatches to the target-specific implementation; x86_64 uses the System V register
/// convention, every other target uses the AArch64 path.
pub fn emit_hash_flip(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_flip_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_flip ---");
    emitter.label_global("__rt_hash_flip");

    // -- set up the frame and preserve the callee-saved source/destination registers --
    // Stack layout:
    //   [sp, #0]  = insertion-order iterator cursor
    //   [sp, #8]  = source key pointer (or inline integer key payload)
    //   [sp, #16] = source key length (-1 marks an inline integer key)
    //   [sp, #24] = source value_lo
    //   [sp, #32] = source value_hi
    //   [sp, #40] = source value_tag
    //   [sp, #48] = flipped key_lo
    //   [sp, #56] = flipped key_hi
    //   [sp, #64] = saved x19/x20
    //   [sp, #80] = saved x29/x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the hash-flip frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up the hash-flip frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19/x20 for the source and destination tables
    emitter.instruction("mov x19, x0");                                         // x19 = source hash pointer, live across every helper call

    // -- allocate the destination table with headroom for every flipped entry --
    emitter.instruction("ldr x0, [x19]");                                       // x0 = source entry count
    emitter.instruction("lsl x0, x0, #1");                                      // double the entry count to give the destination insertion headroom
    emitter.instruction("mov x9, #16");                                         // x9 = minimum destination bucket count
    emitter.instruction("cmp x0, x9");                                          // compare the derived capacity against the runtime minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp very small sources up to the minimum bucket count
    emitter.instruction("bl __rt_hash_new");                                    // x1 already carries the destination value_type tag from the caller
    emitter.instruction("mov x20, x0");                                         // x20 = destination hash pointer, updated after every insertion
    emitter.instruction("str xzr, [sp, #0]");                                   // iterator cursor = 0 (start from header.head)

    // -- walk the source hash in insertion order --
    emitter.label("__rt_hash_flip_loop");
    emitter.instruction("mov x0, x19");                                         // x0 = source hash pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // fetch the next source entry
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_flip_done");                            // yes - the destination hash is complete
    emitter.instruction("str x0, [sp, #0]");                                    // save the next insertion-order cursor
    emitter.instruction("str x1, [sp, #8]");                                    // save the source key pointer before helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save the source key length before helper calls
    emitter.instruction("str x3, [sp, #24]");                                   // save the source value_lo before helper calls
    emitter.instruction("str x4, [sp, #32]");                                   // save the source value_hi before helper calls
    emitter.instruction("str x5, [sp, #40]");                                   // save the source value_tag before helper calls

    // -- peel boxed Mixed values so the flip sees a concrete payload tag --
    emitter.instruction("cmp x5, #7");                                          // runtime tag 7 = boxed mixed cell
    emitter.instruction("b.ne __rt_hash_flip_tag_ready");                       // concrete tags are already usable
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = boxed mixed pointer for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = concrete tag, x1 = value_lo, x2 = value_hi
    emitter.instruction("str x1, [sp, #24]");                                   // save the unboxed value_lo
    emitter.instruction("str x2, [sp, #32]");                                   // save the unboxed value_hi
    emitter.instruction("str x0, [sp, #40]");                                   // save the unboxed concrete value_tag

    // -- PHP only flips int and string values; everything else warns and is skipped --
    emitter.label("__rt_hash_flip_tag_ready");
    emitter.instruction("ldr x5, [sp, #40]");                                   // x5 = concrete source value_tag
    emitter.instruction("cmp x5, #0");                                          // runtime tag 0 = int, which becomes an inline integer key
    emitter.instruction("b.eq __rt_hash_flip_key_int");                         // integer values flip straight into integer keys
    emitter.instruction("cmp x5, #1");                                          // runtime tag 1 = string, which becomes a normalized key
    emitter.instruction("b.eq __rt_hash_flip_key_str");                         // string values flip into normalized string/integer keys
    emitter.instruction("b __rt_hash_flip_skip");                               // float, bool, null, array, and object values are skipped

    emitter.label("__rt_hash_flip_key_int");
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = integer value that becomes the flipped key
    emitter.instruction("str x9, [sp, #48]");                                   // save the flipped key_lo
    emitter.instruction("mov x9, #-1");                                         // key_hi sentinel marks the flipped key as an integer
    emitter.instruction("str x9, [sp, #56]");                                   // save the flipped key_hi sentinel
    emitter.instruction("b __rt_hash_flip_value");                              // the flipped key is ready; build the flipped value

    emitter.label("__rt_hash_flip_key_str");
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = source string value pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // x2 = source string value length
    emitter.instruction("bl __rt_hash_normalize_key");                          // collapse PHP numeric-string values into integer keys
    emitter.instruction("str x1, [sp, #48]");                                   // save the normalized flipped key_lo
    emitter.instruction("str x2, [sp, #56]");                                   // save the normalized flipped key_hi

    // -- the flipped VALUE is the source key; the destination gets its own copy --
    emitter.label("__rt_hash_flip_value");
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = source key length
    emitter.instruction("cmn x2, #1");                                          // does this entry store an inline integer key?
    emitter.instruction("b.eq __rt_hash_flip_value_int");                       // integer keys are copied inline without heap storage
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source string key pointer
    emitter.instruction("bl __rt_str_persist");                                 // copy the source key so the destination owns its own bytes
    emitter.instruction("mov x3, x1");                                          // x3 = owned flipped value pointer
    emitter.instruction("mov x4, x2");                                          // x4 = owned flipped value length
    emitter.instruction("mov x5, #1");                                          // runtime tag 1 marks the flipped value as a string
    emitter.instruction("b __rt_hash_flip_insert");                             // insert the flipped string-valued entry

    emitter.label("__rt_hash_flip_value_int");
    emitter.instruction("ldr x3, [sp, #8]");                                    // x3 = inline integer key payload becoming the flipped value
    emitter.instruction("mov x4, xzr");                                         // integer flipped values carry no high word
    emitter.instruction("mov x5, xzr");                                         // runtime tag 0 marks the flipped value as an int

    emitter.label("__rt_hash_flip_insert");
    emitter.instruction("mov x0, x20");                                         // x0 = destination hash pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // x1 = flipped key_lo
    emitter.instruction("ldr x2, [sp, #56]");                                   // x2 = flipped key_hi
    emitter.instruction("bl __rt_hash_set");                                    // insert the flipped pair; hash_set persists string keys itself
    emitter.instruction("mov x20, x0");                                         // keep the destination pointer current after possible growth
    emitter.instruction("b __rt_hash_flip_loop");                               // continue with the next source entry

    // -- php-src warns once per skipped entry and keeps flipping the rest --
    emitter.label("__rt_hash_flip_skip");
    abi::emit_symbol_address(emitter, "x1", ARRAY_FLIP_SKIPPED_MESSAGES[0].0);
    emitter.instruction(&format!("mov x2, #{}", skip_message_len()));           // pass the complete skip-warning length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the PHP array_flip skip warning
    emitter.instruction("b __rt_hash_flip_loop");                               // skip this entry and continue flipping

    emitter.label("__rt_hash_flip_done");
    emitter.instruction("mov x0, x20");                                         // return the destination hash pointer
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the hash-flip frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash pointer
}

/// Emits the x86_64 System V variant of `__rt_hash_flip`.
///
/// Mirrors the AArch64 logic exactly; only the register convention differs.
///
/// # ABI notes
/// - `__rt_hash_iter_next` returns the entry key pointer in `rdi`, which doubles as argument
///   zero, so every returned field is spilled to the frame before the next helper call.
/// - `__rt_hash_normalize_key` and `__rt_str_persist` both take `rax`/`rdx` on x86_64 rather
///   than the System V argument registers. `__rt_str_persist`'s own docblock still claims
///   `rdi`/`rdx`, but its body reads `rax` — `__rt_hash_set` calls it the same way.
fn emit_hash_flip_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_flip ---");
    emitter.label_global("__rt_hash_flip");

    // Frame layout:
    //   [rbp - 8]  = source hash pointer
    //   [rbp - 16] = destination hash pointer
    //   [rbp - 24] = insertion-order iterator cursor
    //   [rbp - 32] = source key pointer (or inline integer key payload)
    //   [rbp - 40] = source key length (-1 marks an inline integer key)
    //   [rbp - 48] = source value_lo
    //   [rbp - 56] = source value_hi
    //   [rbp - 64] = source value_tag
    //   [rbp - 72] = flipped key_lo
    //   [rbp - 80] = flipped key_hi
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-flip spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the flip bookkeeping
    emitter.instruction("sub rsp, 96");                                         // reserve aligned spill slots so nested helper calls stay 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source hash pointer across every helper call

    // -- allocate the destination table with headroom for every flipped entry --
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = source entry count
    emitter.instruction("shl rax, 1");                                          // double the entry count to give the destination insertion headroom
    emitter.instruction("cmp rax, 16");                                         // compare the derived capacity against the runtime minimum
    emitter.instruction("jge __rt_hash_flip_capacity_x86");                     // keep the doubled count when it already meets the minimum
    emitter.instruction("mov rax, 16");                                         // clamp very small sources up to the minimum bucket count
    emitter.label("__rt_hash_flip_capacity_x86");
    emitter.instruction("mov rdi, rax");                                        // rdi = destination bucket count
    emitter.instruction("call __rt_hash_new");                                  // rsi already carries the destination value_type tag from the caller
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination hash pointer across insertions
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // iterator cursor = 0 (start from header.head)

    // -- walk the source hash in insertion order --
    emitter.label("__rt_hash_flip_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = source hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // rsi = current insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // rax=cursor, rdi=key_ptr, rdx=key_len, rcx=lo, r8=hi, r9=tag
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_hash_flip_done_x86");                          // yes - the destination hash is complete
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // spill the key pointer before rdi is reused as argument zero
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // spill the source key length before helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // spill the source value_lo before helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // spill the source value_hi before helper calls
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // spill the source value_tag before helper calls

    // -- peel boxed Mixed values so the flip sees a concrete payload tag --
    emitter.instruction("cmp r9, 7");                                           // runtime tag 7 = boxed mixed cell
    emitter.instruction("jne __rt_hash_flip_tag_ready_x86");                    // concrete tags are already usable
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // rax = boxed mixed pointer for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // rax = concrete tag, rdi = value_lo, rdx = value_hi
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the unboxed value_lo
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the unboxed value_hi
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the unboxed concrete value_tag

    // -- PHP only flips int and string values; everything else warns and is skipped --
    emitter.label("__rt_hash_flip_tag_ready_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // r10 = concrete source value_tag
    emitter.instruction("cmp r10, 0");                                          // runtime tag 0 = int, which becomes an inline integer key
    emitter.instruction("je __rt_hash_flip_key_int_x86");                       // integer values flip straight into integer keys
    emitter.instruction("cmp r10, 1");                                          // runtime tag 1 = string, which becomes a normalized key
    emitter.instruction("je __rt_hash_flip_key_str_x86");                       // string values flip into normalized string/integer keys
    emitter.instruction("jmp __rt_hash_flip_skip_x86");                         // float, bool, null, array, and object values are skipped

    emitter.label("__rt_hash_flip_key_int_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // r10 = integer value that becomes the flipped key
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the flipped key_lo
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // key_hi sentinel marks the flipped key as an integer
    emitter.instruction("jmp __rt_hash_flip_value_x86");                        // the flipped key is ready; build the flipped value

    emitter.label("__rt_hash_flip_key_str_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // rax = source string value pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // rdx = source string value length
    emitter.instruction("call __rt_hash_normalize_key");                        // collapse PHP numeric-string values into integer keys
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the normalized flipped key_lo
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save the normalized flipped key_hi

    // -- the flipped VALUE is the source key; the destination gets its own copy --
    emitter.label("__rt_hash_flip_value_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 40], -1");                        // does this entry store an inline integer key?
    emitter.instruction("je __rt_hash_flip_value_int_x86");                     // integer keys are copied inline without heap storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // rax = source string key pointer (str_persist reads rax)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // rdx = source string key length
    emitter.instruction("call __rt_str_persist");                               // copy the source key so the destination owns its own bytes
    emitter.instruction("mov rcx, rax");                                        // rcx = owned flipped value pointer
    emitter.instruction("mov r8, rdx");                                         // r8 = owned flipped value length
    emitter.instruction("mov r9, 1");                                           // runtime tag 1 marks the flipped value as a string
    emitter.instruction("jmp __rt_hash_flip_insert_x86");                       // insert the flipped string-valued entry

    emitter.label("__rt_hash_flip_value_int_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // rcx = inline integer key payload becoming the flipped value
    emitter.instruction("xor r8d, r8d");                                        // integer flipped values carry no high word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 marks the flipped value as an int

    emitter.label("__rt_hash_flip_insert_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // rsi = flipped key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // rdx = flipped key_hi
    emitter.instruction("call __rt_hash_set");                                  // insert the flipped pair; hash_set persists string keys itself
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the destination pointer current after possible growth
    emitter.instruction("jmp __rt_hash_flip_loop_x86");                         // continue with the next source entry

    // -- php-src warns once per skipped entry and keeps flipping the rest --
    emitter.label("__rt_hash_flip_skip_x86");
    abi::emit_symbol_address(emitter, "rdi", ARRAY_FLIP_SKIPPED_MESSAGES[0].0);
    emitter.instruction(&format!("mov esi, {}", skip_message_len()));           // pass the complete skip-warning length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the PHP array_flip skip warning
    emitter.instruction("jmp __rt_hash_flip_loop_x86");                         // skip this entry and continue flipping

    emitter.label("__rt_hash_flip_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination hash pointer in rax
    emitter.instruction("add rsp, 96");                                         // release the hash-flip spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return with rax = destination hash pointer
}
