//! Purpose:
//! Tracks PHP's next integer hash key and emits shared checked append-key lookup.
//!
//! Called from:
//! - Hash insertion, native Mixed append, EIR append, and query registration adapters.
//!
//! Key details:
//! - The signed counter survives deletion and saturates at PHP_INT_MAX.
//! - A nonthrowing probe lets query registration stop a field on append exhaustion.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use super::hash_layout::NEXT_INDEX_OFFSET;

/// Records a newly inserted integer key, preserving all registers except the documented scratch set.
/// ARM uses x13-x15; x86 uses r11/r13/r14. The caller supplies a unique branch-label prefix.
pub(super) fn record_insert(emitter: &mut Emitter, hash: &str, entry: &str, prefix: &str) {
    let done = format!("{prefix}_next_index_done");
    if emitter.target.arch == Arch::AArch64 {
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

/// Emits nonthrowing and required lookups, accepting a C-ABI hash pointer without taking ownership.
/// The probe returns index/available in x0/x1 or rax/rdx; the required entry returns the index or Error.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
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
    emitter.label_global("__rt_hash_next_index");
    emitter.instruction(if arm { "stp x29, x30, [sp, #-16]!" } else { "sub rsp, 8" }); // preserve linkage and align the shared probe
    abi::emit_call_label(emitter, "__rt_hash_try_next_index");
    emitter.instruction(if arm { "ldp x29, x30, [sp], #16" } else { "add rsp, 8" }); // restore the caller before any exception unwinding
    emitter.instruction(if arm { "cbz x1, __rt_hash_append_error" } else { "test edx, edx" }); // inspect availability for ordinary PHP append
    if !arm { emitter.instruction("jz __rt_hash_append_error"); }               // saturated occupied keys raise PHP's catchable Error
    emitter.instruction("ret");                                                 // return the available index to EIR before value ownership transfer
    super::hash_append_error::emit(emitter);
}
