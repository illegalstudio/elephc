//! Purpose:
//! Emits `__rt_array_rand_boxed`, `array_rand()` over any array layout and any `$num`, returning
//! php's key or list of keys as a boxed Mixed value.
//!
//! Called from:
//! - The typed ArrayRand backend, for every call the indexed `__rt_array_rand` fast path does not
//!   serve: an associative or boxed array, or a `$num` other than a literal 1.
//!
//! Key details:
//! - A transcription of php-src's `php_array_pick_keys` (ext/standard/array.c), drawing from
//!   `__rt_mt_uniform` so a seeded `mt_srand()` picks php's keys. One key: a single draw in
//!   `[0, count)`, then the key at that position. Several: draws in `[0, count)` until `$num`
//!   distinct positions are marked, the marking inverted when `$num` exceeds half the count (php
//!   then marks the positions to LEAVE), then the source keys in order whose mark differs from
//!   the inversion.
//! - The marks live in a scratch hash keyed by position, released before returning. Integer keys
//!   are stored as integers and string keys as persisted copies, in a fresh list.
//! - Errors return 0 with a code in the second integer result register (aarch64 x1, x86_64
//!   rdx): 1 for an empty array, 2 for a `$num` outside `[1, count]`, 3 for a non-array operand.
//!   Nothing is allocated on those paths.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Error code: the array is empty.
pub const ARRAY_RAND_EMPTY: i64 = 1;
/// Error code: `$num` is outside `[1, count]`.
pub const ARRAY_RAND_BAD_NUM: i64 = 2;
/// Error code: the operand is not an array.
pub const ARRAY_RAND_NOT_ARRAY: i64 = 3;

const NAME: &str = "__rt_array_rand_boxed";

// Frame, as offsets below the frame pointer.
const FRAME: usize = 112;
const SRC: usize = 8;
const NUM: usize = 16;
const COUNT: usize = 24;
const SET: usize = 32;
const RESULT: usize = 40;
const NEGATE: usize = 48;
const CURSOR: usize = 56;
const INDEX: usize = 64;
const NEXT_KEY: usize = 72;
const KEY_LO: usize = 80;
const KEY_HI: usize = 88;
const PICK: usize = 96;

/// Borrows the array cell in ABI arg 0 and `$num` in arg 1; returns an owned Mixed box holding
/// the key (one key) or the list of keys, or 0 with an error code (see the module docs).
pub fn emit_array_rand_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{NAME}_{suffix}");
    emitter.blank();
    emitter.label_global(NAME);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, arg(emitter, 0), SRC);
    abi::store_at_offset(emitter, arg(emitter, 1), NUM);

    // -- validate: an array, not empty --
    abi::load_at_offset(emitter, result, SRC);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub x9, x0, #4");                              // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp x9, #1");                                  // only PHP arrays may be sampled
            emitter.instruction(&format!("b.hi {}", label("not_array")));       // reject scalar, object and null payloads
            abi::store_at_offset_scratch(emitter, "x1", SRC, "x10");
            emitter.instruction("ldr x9, [x1]");                                // both layouts begin with their live count
        }
        Arch::X86_64 => {
            emitter.instruction("lea r10, [rax - 4]");                          // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp r10, 1");                                  // only PHP arrays may be sampled
            emitter.instruction(&format!("ja {}", label("not_array")));         // reject scalar, object and null payloads
            abi::store_at_offset(emitter, "rdi", SRC);
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // both layouts begin with their live count
        }
    }
    let count = scratch(emitter);
    abi::store_at_offset(emitter, count, COUNT);
    emit_branch_if_zero(emitter, count, &label("empty"));

    // -- one key: a single draw, then the key at that position --
    abi::load_at_offset(emitter, result, NUM);
    emit_branch_if_equals(emitter, result, 1, &label("one"));

    // -- several keys: 1 <= $num <= count, inverted past half the count --
    let num = scratch(emitter);
    abi::load_at_offset(emitter, num, NUM);
    abi::load_at_offset(emitter, result, COUNT);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x9, #1");                                  // $num must be at least one
            emitter.instruction(&format!("b.lt {}", label("bad_num")));         // php's ValueError
            emitter.instruction("cmp x9, x0");                                  // and at most the count
            emitter.instruction(&format!("b.gt {}", label("bad_num")));         // php's ValueError
            emitter.instruction("lsr x10, x0, #1");                             // count >> 1
            emitter.instruction("cmp x9, x10");                                 // more than half the keys requested?
            emitter.instruction("cset x11, gt");                                // then mark the keys to leave instead
            emitter.instruction("sub x12, x0, x9");                             // count - $num
            emitter.instruction("csel x9, x12, x9, gt");                        // php marks count - $num positions then
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r9, 1");                                   // $num must be at least one
            emitter.instruction(&format!("jl {}", label("bad_num")));           // php's ValueError
            emitter.instruction("cmp r9, rax");                                 // and at most the count
            emitter.instruction(&format!("jg {}", label("bad_num")));           // php's ValueError
            emitter.instruction("mov r10, rax");                                // copy the count
            emitter.instruction("shr r10, 1");                                  // count >> 1
            emitter.instruction("xor r11d, r11d");                              // assume the marks are the keys to keep
            emitter.instruction("cmp r9, r10");                                 // more than half the keys requested?
            emitter.instruction("setg r11b");                                   // then mark the keys to leave instead
            emitter.instruction("mov r8, rax");                                 // copy the count
            emitter.instruction("sub r8, r9");                                  // count - $num
            emitter.instruction("cmp r11, 0");                                  // inverted?
            emitter.instruction("cmovne r9, r8");                               // php marks count - $num positions then
        }
    }
    abi::store_at_offset(emitter, num, NUM);
    let negate = match emitter.target.arch {
        Arch::AArch64 => "x11",
        Arch::X86_64 => "r11",
    };
    abi::store_at_offset_scratch(emitter, negate, NEGATE, scratch_store(emitter));
    abi::emit_load_int_immediate(emitter, arg(emitter, 0), 8);
    abi::emit_load_int_immediate(emitter, arg(emitter, 1), 0);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::store_at_offset(emitter, result, SET);

    // -- draw positions until $num distinct ones are marked --
    emitter.label(&label("draw"));
    abi::load_at_offset(emitter, result, NUM);
    abi::emit_branch_if_int_result_zero(emitter, &label("collect"));
    emit_draw_position(emitter);
    abi::store_at_offset(emitter, result, PICK);
    abi::load_at_offset(emitter, arg(emitter, 0), SET);
    abi::load_at_offset(emitter, arg(emitter, 1), PICK);
    abi::emit_load_int_immediate(emitter, arg(emitter, 2), -1);
    abi::emit_call_label(emitter, "__rt_hash_get");
    abi::emit_branch_if_int_result_nonzero(emitter, &label("draw"));
    abi::load_at_offset(emitter, arg(emitter, 0), SET);
    abi::load_at_offset(emitter, arg(emitter, 1), PICK);
    abi::emit_load_int_immediate(emitter, arg(emitter, 2), -1);
    abi::emit_load_int_immediate(emitter, arg(emitter, 3), 1);
    abi::emit_load_int_immediate(emitter, arg(emitter, 4), 0);
    abi::emit_load_int_immediate(emitter, arg(emitter, 5), 0);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::store_at_offset(emitter, result, SET);
    abi::load_at_offset(emitter, result, NUM);
    emit_add_immediate(emitter, result, -1);
    abi::store_at_offset(emitter, result, NUM);
    abi::emit_jump(emitter, &label("draw"));

    // -- the keys, in order, whose mark differs from the inversion --
    emitter.label(&label("collect"));
    abi::emit_load_int_immediate(emitter, arg(emitter, 0), 8);
    abi::emit_load_int_immediate(emitter, arg(emitter, 1), 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::store_at_offset(emitter, result, RESULT);
    abi::emit_store_zero_to_local_slot(emitter, CURSOR);
    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    abi::emit_store_zero_to_local_slot(emitter, NEXT_KEY);
    emitter.label(&label("walk"));
    emit_iter_next(emitter, &label("walked"));
    abi::load_at_offset(emitter, arg(emitter, 0), SET);
    abi::load_at_offset(emitter, arg(emitter, 1), INDEX);
    abi::emit_load_int_immediate(emitter, arg(emitter, 2), -1);
    abi::emit_call_label(emitter, "__rt_hash_get");
    let negate_reg = scratch(emitter);
    abi::load_at_offset(emitter, negate_reg, NEGATE);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // was this position marked?
            emitter.instruction("cset x0, ne");                                 // as a boolean
            emitter.instruction("eor x0, x0, x9");                              // php keeps mark XOR inversion
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // was this position marked?
            emitter.instruction("setne al");                                    // as a boolean
            emitter.instruction("movzx eax, al");                               // widen the boolean
            emitter.instruction("xor rax, r9");                                 // php keeps mark XOR inversion
        }
    }
    abi::emit_branch_if_int_result_zero(emitter, &label("next"));
    emit_key_value(emitter, &label("keep"));
    abi::load_at_offset(emitter, arg(emitter, 0), RESULT);
    abi::load_at_offset(emitter, arg(emitter, 1), NEXT_KEY);
    abi::emit_load_int_immediate(emitter, arg(emitter, 2), -1);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::store_at_offset(emitter, result, RESULT);
    abi::load_at_offset(emitter, result, NEXT_KEY);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, NEXT_KEY);
    emitter.label(&label("next"));
    abi::load_at_offset(emitter, result, INDEX);
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, INDEX);
    abi::emit_jump(emitter, &label("walk"));

    // -- box the list; the box takes over the creation reference --
    emitter.label(&label("walked"));
    abi::load_at_offset(emitter, result, SET);
    abi::emit_call_label(emitter, "__rt_decref_hash");
    emit_mixed_from(emitter, 5, RESULT, None);
    abi::store_at_offset(emitter, result, PICK);
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_call_label(emitter, "__rt_decref_hash");
    abi::load_at_offset(emitter, result, PICK);
    abi::emit_jump(emitter, &label("return"));

    // -- one key: draw a position, walk to it, box its key --
    emitter.label(&label("one"));
    emit_draw_position(emitter);
    abi::store_at_offset(emitter, result, PICK);
    abi::emit_store_zero_to_local_slot(emitter, CURSOR);
    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emitter.label(&label("seek"));
    emit_iter_next(emitter, &label("empty"));
    abi::load_at_offset(emitter, result, INDEX);
    let pick = scratch(emitter);
    abi::load_at_offset(emitter, pick, PICK);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x9");                                  // the drawn position?
            emitter.instruction(&format!("b.eq {}", label("found")));           // box its key
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r9");                                 // the drawn position?
            emitter.instruction(&format!("je {}", label("found")));             // box its key
        }
    }
    emit_add_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, INDEX);
    abi::emit_jump(emitter, &label("seek"));
    emitter.label(&label("found"));
    abi::load_at_offset(emitter, result, KEY_HI);
    emit_branch_if_equals(emitter, result, -1, &label("int_key"));
    emit_mixed_from(emitter, 1, KEY_LO, Some(KEY_HI));
    abi::emit_jump(emitter, &label("return"));
    emitter.label(&label("int_key"));
    emit_mixed_from(emitter, 0, KEY_LO, None);
    abi::emit_jump(emitter, &label("return"));

    // -- errors: zero, with the code in the second result word --
    let code = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdx",
    };
    for (suffix, value) in [
        ("empty", ARRAY_RAND_EMPTY),
        ("bad_num", ARRAY_RAND_BAD_NUM),
        ("not_array", ARRAY_RAND_NOT_ARRAY),
    ] {
        emitter.label(&label(suffix));
        abi::emit_load_int_immediate(emitter, code, value);
        abi::emit_load_int_immediate(emitter, result, 0);
        abi::emit_jump(emitter, &label("return"));
    }
    emitter.label(&label("return"));
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Draws a position in `[0, count)` from the Mersenne Twister sampler into the result register.
fn emit_draw_position(emitter: &mut Emitter) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), COUNT);
    abi::emit_call_label(emitter, "__rt_mt_uniform");
}

/// Advances over the source, branching to `done` when exhausted; spills the cursor and the key.
fn emit_iter_next(emitter: &mut Emitter, done: &str) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), SRC);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    let (key_lo, key_hi) = match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // minus one signals iterator exhaustion
            emitter.instruction(&format!("b.eq {done}"));                       // the whole array has been visited
            ("x1", "x2")
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // minus one signals iterator exhaustion
            emitter.instruction(&format!("je {done}"));                         // the whole array has been visited
            ("rcx", "rdx")
        }
    };
    let store = scratch_store(emitter);
    abi::store_at_offset_scratch(emitter, result, CURSOR, store);
    abi::store_at_offset_scratch(emitter, key_lo, KEY_LO, store);
    abi::store_at_offset_scratch(emitter, key_hi, KEY_HI, store);
}

/// Loads the current key as a list value into ABI args 3/4/5 (low, high, tag): an integer key as
/// itself, a string key as a persisted copy the result owns. Falls through to `kept`.
fn emit_key_value(emitter: &mut Emitter, kept: &str) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let string = format!("{kept}_string");
    abi::load_at_offset(emitter, result, KEY_HI);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // minus one marks an integer key
            emitter.instruction(&format!("b.ne {string}"));                     // string keys are copied
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // minus one marks an integer key
            emitter.instruction(&format!("jne {string}"));                      // string keys are copied
        }
    }
    abi::load_at_offset(emitter, arg(emitter, 3), KEY_LO);
    abi::emit_load_int_immediate(emitter, arg(emitter, 4), 0);
    abi::emit_load_int_immediate(emitter, arg(emitter, 5), 0);
    abi::emit_jump(emitter, kept);
    emitter.label(&string);
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, ptr, KEY_LO);
    abi::load_at_offset(emitter, len, KEY_HI);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::emit_reg_move(emitter, arg(emitter, 3), ptr);
    abi::emit_reg_move(emitter, arg(emitter, 4), len);
    abi::emit_load_int_immediate(emitter, arg(emitter, 5), 1);
    emitter.label(kept);
}

/// Boxes `(tag, slot low, slot high or 0)` with `__rt_mixed_from_value`, which persists a string
/// and retains a container; the box lands in the result register.
fn emit_mixed_from(emitter: &mut Emitter, tag: i64, low: usize, high: Option<usize>) {
    let (tag_reg, low_reg, high_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rax", "rdi", "rsi"),
    };
    abi::load_at_offset(emitter, low_reg, low);
    match high {
        Some(slot) => abi::load_at_offset(emitter, high_reg, slot),
        None => abi::emit_load_int_immediate(emitter, high_reg, 0),
    }
    abi::emit_load_int_immediate(emitter, tag_reg, tag);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Branches when `reg` holds zero.
fn emit_branch_if_zero(emitter: &mut Emitter, reg: &str, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbz {reg}, {target}")), // an empty array
        Arch::X86_64 => {
            emitter.instruction(&format!("test {reg}, {reg}"));                 // is the count zero?
            emitter.instruction(&format!("jz {target}"));                       // an empty array
        }
    }
}

/// Branches when `reg` equals the small signed `value`.
fn emit_branch_if_equals(emitter: &mut Emitter, reg: &str, value: i64, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 if value < 0 => {
            emitter.instruction(&format!("cmn {reg}, #{}", -value));            // compare with the negative constant
            emitter.instruction(&format!("b.eq {target}"));                     // take the branch on equality
        }
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {reg}, #{value}"));               // compare with the constant
            emitter.instruction(&format!("b.eq {target}"));                     // take the branch on equality
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {reg}, {value}"));                // compare with the constant
            emitter.instruction(&format!("je {target}"));                       // take the branch on equality
        }
    }
}

/// Adds a small signed immediate to `reg` in place.
fn emit_add_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    match emitter.target.arch {
        Arch::AArch64 if value >= 0 => emitter.instruction(&format!("add {reg}, {reg}, #{value}")), // advance the counter
        Arch::AArch64 => emitter.instruction(&format!("sub {reg}, {reg}, #{}", -value)),           // step the counter back
        Arch::X86_64 => emitter.instruction(&format!("add {reg}, {value}")),                       // adjust the counter in place
    }
}

/// The scratch register holding counts and marks between helper calls.
fn scratch(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r9",
    }
}

/// A scratch register for frame-slot stores, distinct from every value being stored.
fn scratch_store(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r11",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The picker draws from the Mersenne Twister sampler, walks any layout, and releases its
    /// scratch set on every target.
    #[test]
    fn array_rand_picker_draws_from_the_twister_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_rand_boxed(&mut emitter);
            let asm = emitter.output();
            for helper in [
                "__rt_mt_uniform",
                "__rt_array_iter_next",
                "__rt_hash_get",
                "__rt_hash_set",
                "__rt_decref_hash",
                "__rt_mixed_from_value",
            ] {
                assert!(asm.contains(helper), "{target}: {helper}");
            }
            assert!(!asm.contains("__rt_random_uniform"), "{target}: never the CSPRNG chain");
        }
    }
}
