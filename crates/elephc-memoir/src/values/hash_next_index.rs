//! Purpose:
//! Tracks PHP's next integer hash key and emits shared checked append-key lookup.
//!
//! Called from:
//! - Hash insertion, native Mixed append, EIR append, and query registration adapters.
//!
//! Key details:
//! - The signed counter survives unset, but pop rewinds its immediate predecessor.
//! - Saturation at PHP_INT_MAX is preserved when popping that maximum key.
//! - A nonthrowing probe lets query registration stop a field on append exhaustion.

use crate::{Arch, Emitter, abi};
use crate::hash_layout::NEXT_INDEX_OFFSET;

/// Records a newly inserted integer key, preserving all registers except the documented scratch set.
/// ARM uses x13-x15; x86 uses r11/r13/r14. The caller supplies a unique branch-label prefix.
pub fn record_insert(emitter: &mut impl Emitter, hash: &str, entry: &str, prefix: &str) {
    let done = format!("{prefix}_next_index_done");
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("ldr x13, [{entry}, #16]"));               // inspect the normalized key length
        emitter.instruction("cmn x13, #1");                                     // integer keys carry the negative-one length sentinel
        emitter.instruction(&format!("b.ne {done}"));                           // string keys leave automatic indexing unchanged
        emitter.instruction(&format!("ldr x13, [{entry}, #8]"));                // recover the newly inserted signed integer key
        emitter.instruction(&format!("ldr x15, [{hash}, #{NEXT_INDEX_OFFSET}]")); // read the persistent insertion history
        emitter.instruction("cmp x13, x15");                                    // compare signed keys, including PHP's negative starting indices
        emitter.instruction(&format!("b.lt {done}"));                           // older keys never rewind the next index
        emitter.instruction("adds x14, x13, #1");                               // advance unless PHP_INT_MAX would overflow
        emitter.instruction("csel x14, x13, x14, vs");                          // saturate at the largest signed integer
        emitter.instruction(&format!("str x14, [{hash}, #{NEXT_INDEX_OFFSET}]")); // retain the next index independently of occupied entries
    } else {
        emitter.instruction(&format!("cmp QWORD PTR [{entry} + 16], -1"));      // distinguish normalized integer keys from string keys
        emitter.instruction(&format!("jne {done}"));                            // string keys do not advance automatic indexing
        emitter.instruction(&format!("mov r13, QWORD PTR [{entry} + 8]"));      // recover the inserted signed integer key
        emitter.instruction(&format!("cmp r13, QWORD PTR [{hash} + {NEXT_INDEX_OFFSET}]")); // compare against the persistent signed counter
        emitter.instruction(&format!("jl {done}"));                             // preserve history when inserting a smaller key
        emitter.instruction("mov r14, r13");                                    // preserve the original key for overflow saturation
        emitter.instruction("add r14, 1");                                      // compute the successor with signed overflow flags
        emitter.instruction("cmovo r14, r13");                                  // keep PHP_INT_MAX instead of wrapping negative
        emitter.instruction(&format!("mov QWORD PTR [{hash} + {NEXT_INDEX_OFFSET}], r14")); // publish the persistent next index
    }
    emitter.label(&done);
}

/// Rewinds the append counter only when pop removes its integer predecessor.
/// Preserves the hash/key/length argument registers; ARM uses x9/x10, x86 uses r10/r11.
pub fn rewind_after_pop(
    emitter: &mut impl Emitter, hash: &str, key: &str, key_len: &str, prefix: &str,
) {
    let done = format!("{prefix}_next_index_done");
    match emitter.arch() {
        Arch::AArch64 => {
            // -- rewind the next key only when the popped integer is its immediate predecessor --
            emitter.instruction(&format!("cmn {key_len}, #1"));                 // only integer tail keys can rewind automatic indexing
            emitter.instruction(&format!("b.ne {done}"));                       // popping a string key preserves numeric insertion history
            emitter.instruction(&format!("ldr x9, [{hash}, #{NEXT_INDEX_OFFSET}]")); // read the current next automatic integer key
            emitter.instruction("sub x10, x9, #1");                             // match PHP's immediate predecessor rule, including saturation
            emitter.instruction(&format!("cmp {key}, x10"));                    // older numeric tail entries do not rewind a higher counter
            emitter.instruction(&format!("b.ne {done}"));                       // retain history unless this is the counter's predecessor
            emitter.instruction(&format!("str {key}, [{hash}, #{NEXT_INDEX_OFFSET}]")); // let the next append reuse the popped numeric slot
        }
        Arch::X86_64 => {
            // -- preserve append history unless the popped integer directly precedes it --
            emitter.instruction(&format!("cmp {key_len}, -1"));                 // only integer tail keys can rewind automatic indexing
            emitter.instruction(&format!("jne {done}"));                        // popping a string key preserves numeric insertion history
            emitter.instruction(&format!("mov r10, QWORD PTR [{hash} + {NEXT_INDEX_OFFSET}]")); // read the persistent next automatic integer key
            emitter.instruction("lea r11, [r10 - 1]");                          // match PHP's immediate predecessor rule, including saturation
            emitter.instruction(&format!("cmp {key}, r11"));                    // lower numeric tail entries preserve a higher insertion history
            emitter.instruction(&format!("jne {done}"));                        // rewind only when this key directly precedes the counter
            emitter.instruction(&format!("mov QWORD PTR [{hash} + {NEXT_INDEX_OFFSET}], {key}")); // let the next append reuse the popped numeric slot
        }
    }
    emitter.label(&done);
}

/// Emits the nonthrowing index/availability probe for a borrowed hash.
/// Returns index/available in x0/x1 or rax/rdx.
pub fn emit_try_next_index(emitter: &mut impl Emitter) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.label_global("__rt_hash_try_next_index");
    if arm {
        emitter.instruction(&format!("ldr x9, [x0, #{NEXT_INDEX_OFFSET}]"));    // read insertion history without a capacity scan
        abi::emit_load_int_immediate(emitter, "x10", i64::MIN);
        emitter.instruction("cmp x9, x10");                                     // a never-indexed hash starts at integer zero
        emitter.instruction("csel x9, xzr, x9, eq");                            // preserve genuine negative counters after their first insertion
        abi::emit_load_int_immediate(emitter, "x10", i64::MAX);
        emitter.instruction("cmp x9, x10");                                     // only the saturated counter can name an occupied key
        emitter.instruction("b.eq __rt_hash_try_next_index_saturated");         // check PHP_INT_MAX before allowing an append
        emitter.instruction("mov x0, x9");                                      // return the available signed append index
        emitter.instruction("mov x1, #1");                                      // report success without mutating or separating the hash
    } else {
        emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {NEXT_INDEX_OFFSET}]")); // read insertion history without scanning entries
        abi::emit_load_int_immediate(emitter, "r10", i64::MIN);
        emitter.instruction("xor r11d, r11d");                                  // prepare zero for a never-indexed hash
        emitter.instruction("cmp rax, r10");                                    // detect the initial signed sentinel
        emitter.instruction("cmove rax, r11");                                  // otherwise retain the signed counter, including negatives
        abi::emit_load_int_immediate(emitter, "r10", i64::MAX);
        emitter.instruction("cmp rax, r10");                                    // only saturation requires an occupancy lookup
        emitter.instruction("je __rt_hash_try_next_index_saturated");           // distinguish a reusable maximum key from exhaustion
        emitter.instruction("mov edx, 1");                                      // return availability beside the append index
    }
    emitter.instruction("ret");                                                 // return a borrowed, non-mutating key decision
    emitter.label("__rt_hash_try_next_index_saturated");
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve linkage for the exceptional maximum-key lookup
        emitter.instruction("mov x1, x10");                                     // look up PHP_INT_MAX in the current table
        emitter.instruction("mov x2, #-1");                                     // select integer-key lookup
        emitter.instruction("bl __rt_hash_get");                                // inspect occupancy without invoking PHP
        emitter.instruction("eor x1, x0, #1");                                  // a missing maximum key is available after unset
        abi::emit_load_int_immediate(emitter, "x0", i64::MAX);
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore linkage while preserving both result words
    } else {
        emitter.instruction("sub rsp, 8");                                      // align the nested maximum-key lookup
        emitter.instruction("mov rsi, r10");                                    // look up PHP_INT_MAX in the current table
        emitter.instruction("mov rdx, -1");                                     // select integer-key lookup
        emitter.instruction("call __rt_hash_get");                              // inspect occupancy without separating shared storage
        emitter.instruction("xor eax, 1");                                      // invert the lookup's found flag
        emitter.instruction("mov edx, eax");                                    // report whether the maximum key can be reused
        abi::emit_load_int_immediate(emitter, "rax", i64::MAX);
        emitter.instruction("add rsp, 8");                                      // restore the caller's stack alignment
    }
    emitter.instruction("ret");                                                 // report exhaustion without throwing for query callers
}

/// Copies exact source history into a freshly rebuilt hash without acquiring PHP owners.
pub fn emit_copy_array_history(emitter: &mut impl Emitter, entry: &str) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.label_global(&emitter.extern_symbol(entry));
    if arm {
        emitter.instruction("sub sp, sp, #32");                                 // retain destination, source history, and linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // preserve the caller across reference resolution
        emitter.instruction("str x1, [sp]");                                    // retain the freshly reconstructed destination
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align calls
        emitter.instruction("mov rbp, rsp");                                    // establish the metadata callback frame
        emitter.instruction("sub rsp, 16");                                     // retain destination and source history
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // retain the reconstructed destination
        emitter.instruction("mov rax, rdi");                                    // adapt the source to private dereferencing
    }
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    emitter.instruction(if arm { "cbz x0, __rt_eval_copy_index_invalid" } else { "test rax, rax" }); // validate the borrowed source cell
    if !arm { emitter.instruction("jz __rt_eval_copy_index_invalid"); }         // reject a missing source
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r11, QWORD PTR [rax]" }); // inspect the source representation
    emitter.instruction(if arm { "ldr x0, [x0, #8]" } else { "mov rax, QWORD PTR [rax + 8]" }); // borrow source array storage
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r11, 4" });         // indexed arrays preserve their dense length
    emitter.instruction(if arm { "b.eq __rt_eval_copy_index_dense" } else { "je __rt_eval_copy_index_dense" }); // derive history before deleting a dense tail
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r11, 5" });         // associative arrays own the signed counter
    emitter.instruction(if arm { "b.ne __rt_eval_copy_index_invalid" } else { "jne __rt_eval_copy_index_invalid" }); // reject unsupported source values
    crate::layout::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_copy_index_empty");
    let result = emitter.int_result_reg();
    emitter.emit_load_from_address(result, result, NEXT_INDEX_OFFSET);
    emitter.instruction(if arm { "b __rt_eval_copy_index_resolve" } else { "jmp __rt_eval_copy_index_resolve" }); // preserve associative history across destination resolution
    emitter.label("__rt_eval_copy_index_dense");
    crate::layout::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_copy_index_empty");
    super::packed_slots::emit_load_length(emitter, result, result);
    emitter.instruction(if arm { "cbnz x0, __rt_eval_copy_index_resolve" } else { "test rax, rax" }); // preserve the nonempty dense length
    if !arm { emitter.instruction("jnz __rt_eval_copy_index_resolve"); }        // an empty dense source has no integer insertion history
    emitter.label("__rt_eval_copy_index_empty");
    abi::emit_load_int_immediate(emitter, if arm { "x0" } else { "rax" }, i64::MIN);
    emitter.label("__rt_eval_copy_index_resolve");
    emitter.instruction(if arm { "str x0, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], rax" }); // retain exact metadata across destination dereferencing
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // resolve the newly reconstructed cell
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    emitter.instruction(if arm { "cbz x0, __rt_eval_copy_index_invalid" } else { "test rax, rax" }); // require a valid destination cell
    if !arm { emitter.instruction("jz __rt_eval_copy_index_invalid"); }         // reject an absent destination
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r11, QWORD PTR [rax]" }); // inspect destination storage
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r11, 5" });         // arbitrary history requires associative storage
    emitter.instruction(if arm { "b.ne __rt_eval_copy_index_invalid" } else { "jne __rt_eval_copy_index_invalid" }); // preserve unsupported destinations unchanged
    emitter.instruction(if arm { "ldr x0, [x0, #8]" } else { "mov rax, QWORD PTR [rax + 8]" }); // borrow the fresh destination hash
    crate::layout::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_copy_index_invalid");
    emitter.instruction(if arm { "ldr x9, [sp, #8]" } else { "mov r10, QWORD PTR [rsp + 8]" }); // recover the original insertion history
    let history = if arm { "x9" } else { "r10" };
    emitter.emit_store_to_address(history, result, NEXT_INDEX_OFFSET);
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // report successful metadata transfer
    emitter.instruction(if arm { "b __rt_eval_copy_index_done" } else { "jmp __rt_eval_copy_index_done" }); // share callback cleanup
    emitter.label("__rt_eval_copy_index_invalid");
    emitter.instruction(if arm { "mov x0, #2" } else { "mov eax, 2" });         // return unsupported storage without running PHP
    emitter.label("__rt_eval_copy_index_done");
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore caller linkage
        emitter.instruction("add sp, sp, #32");                                 // retire borrowed metadata storage
    } else {
        emitter.instruction("leave");                                           // restore the caller frame and stack
    }
    emitter.instruction("ret");                                                 // return the independent host status
}
