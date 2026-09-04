//! Purpose:
//! Emits `__rt_hash_filter`, `array_filter()` over a source whose KEYS must survive into the
//! result — which, in php, is every source.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `__rt_array_filter` builds a LIST of the survivors, renumbering them from zero. php does not:
//!   MEASURED on `php -n` 8.5.6, `array_filter([0,1,2])` answers `[1=>1, 2=>2]`, and the keys are
//!   real — `isset($result[0])` is false and `foreach` yields 1 and 2. The renumbering was a
//!   silent wrong answer in every callback form, including no callback at all.
//! - Nothing here re-implements hashing or ownership. `__rt_hash_clone_shallow` already produces a
//!   table that owns its own keys and values and preserves insertion order, and `__rt_hash_unset`
//!   already releases one entry's payloads and unlinks it while keeping probe chains intact. So
//!   the filter is: clone, then drop what the predicate rejects. Building a destination by
//!   insertion instead would have meant re-deriving the ownership rules those two encode.
//! - The walk reads the SOURCE and unsets from the DESTINATION, never the table being iterated.
//! - The callback ABI follows the runtime tags, exactly as `__rt_hash_map` does: value tag 1 is a
//!   string, so it travels as a pointer/length pair, and a key with `key_hi == -1` is an integer
//!   rather than a string pair. `ARRAY_FILTER_USE_BOTH` therefore has four argument shapes, and
//!   all four are emitted rather than assumed away — a hash grown from an indexed array has
//!   integer keys, a genuine associative one usually does not.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use super::value_error;

/// The wording php uses for a mode outside the three it defines, shared with `__rt_array_filter`.
const MODE_MSG_LEN: usize = "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH.".len();

/// Emits `__rt_hash_filter`.
///
/// Input:  x0/rdi = predicate address, x1/rsi = source hash, x2/rdx = capture environment
///         (0 when the predicate captures nothing), x3/rcx = `array_filter()` mode
/// Output: x0/rax = a hash holding the entries the predicate kept, under their source keys
pub fn emit_hash_filter(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
    emit_keyed_adapter(emitter);
}

/// Emits `__rt_array_filter_keyed`, the indexed-source entry.
///
/// php's keys survive `array_filter()` whatever the source is, and an indexed array cannot carry
/// them once anything is dropped. `__rt_array_to_hash` already produces the 0..n-1 keyed table
/// that can, so this converts and delegates rather than growing a second filter loop.
///
/// Input:  x0/rdi = predicate, x1/rsi = source indexed array, x2/rdx = environment, x3/rcx = mode
/// Output: x0/rax = a hash under the source's own integer keys
fn emit_keyed_adapter(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_keyed ---");
    emitter.label_global("__rt_array_filter_keyed");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // hold the predicate, environment and mode across the conversion
            emitter.instruction("stp x29, x30, [sp, #32]");
            emitter.instruction("add x29, sp, #32");
            emitter.instruction("stp x0, x2, [sp, #0]");                        // the predicate and its environment
            emitter.instruction("str x3, [sp, #16]");                           // the mode
            // The mode is judged BEFORE the conversion allocates. Converting first and letting
            // `__rt_hash_filter` raise would abandon the hash `__rt_array_to_hash` just built.
            emitter.instruction("cmp x3, #0");
            emitter.instruction("b.eq __rt_afk_mode_ok");
            emitter.instruction("cmp x3, #1");
            emitter.instruction("b.eq __rt_afk_mode_ok");
            emitter.instruction("cmp x3, #2");
            emitter.instruction("b.eq __rt_afk_mode_ok");
            value_error::emit_throw_value_error_aarch64(
                emitter,
                "_array_filter_mode_msg",
                MODE_MSG_LEN,
            );
            emitter.label("__rt_afk_mode_ok");
            emitter.instruction("mov x0, x1");                                  // the list to convert
            emitter.instruction("bl __rt_array_to_hash");                       // integer keys 0..length-1
            emitter.instruction("mov x1, x0");                                  // it becomes the filter's source
            emitter.instruction("ldp x0, x2, [sp, #0]");
            emitter.instruction("ldr x3, [sp, #16]");
            emitter.instruction("bl __rt_hash_filter");
            emitter.instruction("ldp x29, x30, [sp, #32]");
            emitter.instruction("add sp, sp, #48");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 32");                                 // hold the predicate, environment and mode across the conversion
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the predicate
            emitter.instruction("mov QWORD PTR [rbp - 16], rdx");               // its environment
            emitter.instruction("mov QWORD PTR [rbp - 24], rcx");               // the mode
            // The mode is judged BEFORE the conversion allocates; see the AArch64 arm.
            emitter.instruction("cmp rcx, 0");
            emitter.instruction("je __rt_afk_mode_ok_x");
            emitter.instruction("cmp rcx, 1");
            emitter.instruction("je __rt_afk_mode_ok_x");
            emitter.instruction("cmp rcx, 2");
            emitter.instruction("je __rt_afk_mode_ok_x");
            value_error::emit_throw_value_error_x86_64(
                emitter,
                "_array_filter_mode_msg",
                MODE_MSG_LEN,
            );
            emitter.label("__rt_afk_mode_ok_x");
            emitter.instruction("mov rdi, rsi");                                // the list to convert
            emitter.instruction("call __rt_array_to_hash");                     // integer keys 0..length-1
            emitter.instruction("mov rsi, rax");                                // it becomes the filter's source
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
            emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");
            emitter.instruction("call __rt_hash_filter");
            emitter.instruction("add rsp, 32");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits the AArch64 implementation.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_filter ---");
    emitter.label_global("__rt_hash_filter");

    // Frame: [0]=cursor [8]=key_lo [16]=key_hi [24]=val_lo [32]=val_hi [40]=mode [48]=value tag
    //        [64]=saved x21/x22 [80]=saved x19/x20 [96]=saved x29/x30
    emitter.instruction("sub sp, sp, #112");                                    // reserve the filter frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the filter frame pointer
    emitter.instruction("stp x19, x20, [sp, #80]");                             // save callee-saved x19/x20 for the source and destination tables
    emitter.instruction("stp x21, x22, [sp, #64]");                             // save callee-saved x21/x22 for the predicate and its environment
    emitter.instruction("mov x21, x0");                                         // x21 = predicate address, live across every iteration
    emitter.instruction("mov x19, x1");                                         // x19 = source hash, read but never written
    emitter.instruction("mov x22, x2");                                         // x22 = capture environment (0 when unused)
    emitter.instruction("str x3, [sp, #40]");                                   // hold the mode for the per-entry argument setup

    // -- php refuses a mode outside the three it defines, before doing any work --
    emitter.instruction("cmp x3, #0");                                          // ARRAY_FILTER_USE_VALUE
    emitter.instruction("b.eq __rt_hash_filter_mode_ok");
    emitter.instruction("cmp x3, #1");                                          // ARRAY_FILTER_USE_BOTH
    emitter.instruction("b.eq __rt_hash_filter_mode_ok");
    emitter.instruction("cmp x3, #2");                                          // ARRAY_FILTER_USE_KEY
    emitter.instruction("b.eq __rt_hash_filter_mode_ok");
    emitter.instruction("b __rt_hash_filter_bad_mode");
    emitter.label("__rt_hash_filter_mode_ok");

    // -- the destination starts as a full copy that owns its own entries --
    emitter.instruction("mov x0, x19");
    emitter.instruction("bl __rt_hash_clone_shallow");                          // keys re-persisted, values retained, order preserved
    emitter.instruction("mov x20, x0");                                         // x20 = destination, updated whenever unset splits it
    emitter.instruction("str xzr, [sp, #0]");                                   // cursor = 0, the head of the insertion order

    emitter.label("__rt_hash_filter_loop");
    emitter.instruction("mov x0, x19");                                         // walk the SOURCE, never the table being modified
    emitter.instruction("ldr x1, [sp, #0]");
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=cursor x1=key_lo x2=key_hi x3=val_lo x4=val_hi x5=tag
    emitter.instruction("cmn x0, #1");                                          // -1 marks the end of the walk
    emitter.instruction("b.eq __rt_hash_filter_done");
    emitter.instruction("str x0, [sp, #0]");                                    // the next cursor
    emitter.instruction("stp x1, x2, [sp, #8]");                                // the key, needed again if the entry is dropped
    emitter.instruction("stp x3, x4, [sp, #24]");                               // the value, which the predicate call clobbers
    emitter.instruction("str x5, [sp, #48]");                                   // its runtime tag, which selects the argument ABI

    emitter.instruction("ldr x9, [sp, #40]");
    emitter.instruction("cmp x9, #2");
    emitter.instruction("b.eq __rt_hash_filter_key_args");
    emitter.instruction("cmp x9, #1");
    emitter.instruction("b.eq __rt_hash_filter_both_args");

    // -- ARRAY_FILTER_USE_VALUE: the predicate sees the value alone --
    emitter.instruction("cmp x5, #1");                                          // runtime tag 1 = string
    emitter.instruction("b.eq __rt_hash_filter_value_str");
    emitter.instruction("mov x0, x3");                                          // one scalar register
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x1, x22");                                         // the environment rides after the value
    emitter.instruction("b __rt_hash_filter_call");
    emitter.label("__rt_hash_filter_value_str");
    emitter.instruction("mov x0, x3");                                          // pointer
    emitter.instruction("mov x1, x4");                                          // and length
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x2, x22");
    emitter.instruction("b __rt_hash_filter_call");

    // -- ARRAY_FILTER_USE_KEY: the predicate sees the key alone --
    emitter.label("__rt_hash_filter_key_args");
    emitter.instruction("cmn x2, #1");                                          // key_hi == -1 marks an integer key
    emitter.instruction("b.eq __rt_hash_filter_key_int");
    emitter.instruction("mov x0, x1");                                          // string key: pointer
    emitter.instruction("mov x1, x2");                                          // and length
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x2, x22");
    emitter.instruction("b __rt_hash_filter_call");
    emitter.label("__rt_hash_filter_key_int");
    emitter.instruction("mov x0, x1");                                          // integer key in one register
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x1, x22");
    emitter.instruction("b __rt_hash_filter_call");

    // -- ARRAY_FILTER_USE_BOTH: php passes ($value, $key), so four shapes --
    emitter.label("__rt_hash_filter_both_args");
    emitter.instruction("cmp x5, #1");
    emitter.instruction("b.eq __rt_hash_filter_both_value_str");
    emitter.instruction("cmn x2, #1");
    emitter.instruction("b.eq __rt_hash_filter_both_scalar_int");
    emitter.instruction("mov x0, x3");                                          // scalar value, string key: x1/x2 already hold it
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x3, x22");
    emitter.instruction("b __rt_hash_filter_call");
    emitter.label("__rt_hash_filter_both_scalar_int");
    emitter.instruction("mov x0, x3");                                          // scalar value, integer key: x1 already holds it
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x2, x22");
    emitter.instruction("b __rt_hash_filter_call");
    emitter.label("__rt_hash_filter_both_value_str");
    emitter.instruction("cmn x2, #1");
    emitter.instruction("b.eq __rt_hash_filter_both_str_int");
    emitter.instruction("mov x9, x1");                                          // string value, string key
    emitter.instruction("mov x10, x2");
    emitter.instruction("mov x0, x3");
    emitter.instruction("mov x1, x4");
    emitter.instruction("mov x2, x9");
    emitter.instruction("mov x3, x10");
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x4, x22");
    emitter.instruction("b __rt_hash_filter_call");
    emitter.label("__rt_hash_filter_both_str_int");
    emitter.instruction("mov x9, x1");                                          // string value, integer key
    emitter.instruction("mov x0, x3");
    emitter.instruction("mov x1, x4");
    emitter.instruction("mov x2, x9");
    emitter.instruction("cbz x22, __rt_hash_filter_call");
    emitter.instruction("mov x3, x22");

    emitter.label("__rt_hash_filter_call");
    emitter.instruction("blr x21");                                             // the predicate answers truthiness in x0
    emitter.instruction("cbnz x0, __rt_hash_filter_loop");                      // kept: the clone already holds it

    // -- rejected: drop it from the destination, which owns its own copy --
    emitter.instruction("mov x0, x20");
    emitter.instruction("ldr x1, [sp, #8]");                                    // key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // key_hi (-1 marks an integer key)
    emitter.instruction("bl __rt_hash_unset");                                  // releases the entry's payloads and unlinks it
    emitter.instruction("mov x20, x0");                                         // unset splits the table copy-on-write, so re-read it
    emitter.instruction("b __rt_hash_filter_loop");

    emitter.label("__rt_hash_filter_done");
    emitter.instruction("mov x0, x20");
    emitter.instruction("ldp x21, x22, [sp, #64]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #80]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the filter frame
    emitter.instruction("ret");

    emitter.label("__rt_hash_filter_bad_mode");
    value_error::emit_throw_value_error_aarch64(emitter, "_array_filter_mode_msg", MODE_MSG_LEN);
}

/// Emits the x86_64 System V implementation.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_filter ---");
    emitter.label_global("__rt_hash_filter");

    // Frame: [rbp-8]=source [rbp-16]=destination [rbp-24]=cursor [rbp-32]=key_lo [rbp-40]=key_hi
    //        [rbp-48]=predicate [rbp-56]=environment [rbp-64]=mode
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the filter frame
    emitter.instruction("sub rsp, 96");                                         // keep every nested call System V aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // the source hash, read but never written
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // the predicate address
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // the capture environment (0 when unused)
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // the mode

    // -- php refuses a mode outside the three it defines, before doing any work --
    emitter.instruction("cmp rcx, 0");
    emitter.instruction("je __rt_hash_filter_mode_ok_x86");
    emitter.instruction("cmp rcx, 1");
    emitter.instruction("je __rt_hash_filter_mode_ok_x86");
    emitter.instruction("cmp rcx, 2");
    emitter.instruction("je __rt_hash_filter_mode_ok_x86");
    emitter.instruction("jmp __rt_hash_filter_bad_mode_x86");
    emitter.label("__rt_hash_filter_mode_ok_x86");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("call __rt_hash_clone_shallow");                        // keys re-persisted, values retained, order preserved
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the destination, updated whenever unset splits it
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // cursor = 0, the head of the insertion order

    emitter.label("__rt_hash_filter_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // walk the SOURCE, never the table being modified
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_hash_iter_next");                            // rax=cursor rdi=key_lo rdx=key_hi rcx=lo r8=hi r9=tag
    emitter.instruction("cmp rax, -1");
    emitter.instruction("je __rt_hash_filter_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the next cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // the key, needed again if the entry is dropped
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");

    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");
    emitter.instruction("cmp r10, 2");
    emitter.instruction("je __rt_hash_filter_key_args_x86");
    emitter.instruction("cmp r10, 1");
    emitter.instruction("je __rt_hash_filter_both_args_x86");

    // -- ARRAY_FILTER_USE_VALUE --
    emitter.instruction("cmp r9, 1");                                           // runtime tag 1 = string
    emitter.instruction("je __rt_hash_filter_value_str_x86");
    emitter.instruction("mov rdi, rcx");                                        // one scalar register
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // the environment rides after the value
    emitter.instruction("jmp __rt_hash_filter_call_x86");
    emitter.label("__rt_hash_filter_value_str_x86");
    emitter.instruction("mov rdi, rcx");                                        // pointer
    emitter.instruction("mov rsi, r8");                                         // and length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");
    emitter.instruction("jmp __rt_hash_filter_call_x86");

    // -- ARRAY_FILTER_USE_KEY --
    emitter.label("__rt_hash_filter_key_args_x86");
    emitter.instruction("cmp rdx, -1");                                         // key_hi == -1 marks an integer key
    emitter.instruction("je __rt_hash_filter_key_int_x86");
    emitter.instruction("mov rsi, rdx");                                        // string key: length, before rdi is reused
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");
    emitter.instruction("jmp __rt_hash_filter_call_x86");                       // rdi already holds the key pointer
    emitter.label("__rt_hash_filter_key_int_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // rdi already holds the integer key
    emitter.instruction("jmp __rt_hash_filter_call_x86");

    // -- ARRAY_FILTER_USE_BOTH: php passes ($value, $key), so four shapes --
    emitter.label("__rt_hash_filter_both_args_x86");
    emitter.instruction("cmp r9, 1");
    emitter.instruction("je __rt_hash_filter_both_value_str_x86");
    emitter.instruction("cmp rdx, -1");
    emitter.instruction("je __rt_hash_filter_both_scalar_int_x86");
    emitter.instruction("mov r11, rdi");                                        // scalar value, string key
    emitter.instruction("mov rdi, rcx");
    emitter.instruction("mov rsi, r11");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");
    emitter.instruction("jmp __rt_hash_filter_call_x86");
    emitter.label("__rt_hash_filter_both_scalar_int_x86");
    emitter.instruction("mov r11, rdi");                                        // scalar value, integer key
    emitter.instruction("mov rdi, rcx");
    emitter.instruction("mov rsi, r11");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");
    emitter.instruction("jmp __rt_hash_filter_call_x86");
    emitter.label("__rt_hash_filter_both_value_str_x86");
    emitter.instruction("cmp rdx, -1");
    emitter.instruction("je __rt_hash_filter_both_str_int_x86");
    emitter.instruction("mov r11, rdi");                                        // string value, string key
    emitter.instruction("mov r10, rdx");
    emitter.instruction("mov rdi, rcx");
    emitter.instruction("mov rsi, r8");
    emitter.instruction("mov rdx, r11");
    emitter.instruction("mov rcx, r10");
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");
    emitter.instruction("jmp __rt_hash_filter_call_x86");
    emitter.label("__rt_hash_filter_both_str_int_x86");
    emitter.instruction("mov r11, rdi");                                        // string value, integer key
    emitter.instruction("mov rdi, rcx");
    emitter.instruction("mov rsi, r8");
    emitter.instruction("mov rdx, r11");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");

    emitter.label("__rt_hash_filter_call_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // the predicate, in a caller-saved scratch register
    emitter.instruction("call r10");                                            // it answers truthiness in rax
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_hash_filter_loop_x86");                       // kept: the clone already holds it

    // -- rejected: drop it from the destination, which owns its own copy --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // key_hi (-1 marks an integer key)
    emitter.instruction("call __rt_hash_unset");                                // releases the entry's payloads and unlinks it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // unset splits the table copy-on-write, so re-read it
    emitter.instruction("jmp __rt_hash_filter_loop_x86");

    emitter.label("__rt_hash_filter_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
    emitter.instruction("add rsp, 96");                                         // release the filter frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");

    emitter.label("__rt_hash_filter_bad_mode_x86");
    value_error::emit_throw_value_error_x86_64(emitter, "_array_filter_mode_msg", MODE_MSG_LEN);
}
