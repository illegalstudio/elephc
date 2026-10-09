//! Purpose:
//! Emits `__rt_array_combine_boxed`, the `array_combine()` / `array_fill_keys()` builder over any
//! key and value layout.
//!
//! Called from:
//! - The typed ArrayCombine and ArrayFillKeys backends, for the shapes their string-keyed typed
//!   helpers cannot read (integer or boxed keys, string values, associative or boxed operands).
//!
//! Key details:
//! - php turns each key element into an array key the way `php_array_combine` and
//!   `array_fill_keys` do: an integer stays an integer key, anything else is rendered as
//!   `(string)` would and then normalized, so `"5"` and `true` become integer keys and `2.5`
//!   the string key `"2.5"`. The rendering is `__rt_mixed_cast_string`, released after the insert.
//! - Values are copied as `__rt_array_to_hash` copies them: a string is duplicated, heap values
//!   are retained. A later duplicate key overwrites the earlier value, as in php.
//! - Rendering a key can run user code (`__toString`, or the error handler of the "Array to string
//!   conversion" warning), so the scan runs under `__rt_cleanup_invoke` like
//!   `__rt_array_set_op_boxed`: both arrays and the fill value are retained first, and a throw
//!   releases the partial result before it propagates.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::exceptions};

/// Mode argument: values come from a second array, walked in step with the keys.
pub const MODE_COMBINE: i64 = 0;
/// Mode argument: every key gets the same borrowed value cell.
pub const MODE_FILL: i64 = 1;
/// Position reported for `array_combine()` operands of different lengths.
pub const COUNT_MISMATCH: i64 = 3;

const NAME: &str = "__rt_array_combine_boxed";
const BODY: &str = "__rt_array_combine_boxed_body";

// Boundary frame, as offsets below the frame pointer. The context is the address of RESULT; the
// fill value cell occupies three ascending words at `context + 32`.
const FRAME: usize = 96;
const PENDING: usize = 8;
const FILL_HIGH: usize = 16;
const FILL_LOW: usize = 24;
const FILL: usize = 32;
const MODE: usize = 40;
const VALUES: usize = 48;
const KEYS: usize = 56;
const RESULT: usize = 64;
const CTX_RESULT: usize = 0;
const CTX_KEYS: usize = 8;
const CTX_VALUES: usize = 16;
const CTX_MODE: usize = 24;
const CTX_FILL: usize = 32;

// Body frame. Each cell is three ascending words starting at its tag slot.
const BODY_FRAME: usize = 128;
const CONTEXT: usize = 8;
const KEY_CURSOR: usize = 16;
const VALUE_CURSOR: usize = 24;
const KEY_HIGH: usize = 32;
const KEY_LOW: usize = 40;
const KEY: usize = 48;
const VALUE_HIGH: usize = 56;
const VALUE_LOW: usize = 64;
const VALUE: usize = 72;
const NORMAL_LO: usize = 80;
const NORMAL_HI: usize = 88;
const RENDER: usize = 96;
const CONCAT: usize = 104;

/// Borrows the keys cell in ABI arg 0, the values cell (an array for `MODE_COMBINE`, any value
/// for `MODE_FILL`) in arg 1, and the mode in arg 2.
///
/// Returns the owned result hash, or 0 with a position in the second integer result register
/// (aarch64 x1, x86_64 rdx): 1 or 2 for an operand that is not an array, `COUNT_MISMATCH` when
/// the two arrays differ in length. Nothing has been allocated when it returns 0.
pub fn emit_array_combine_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{NAME}_{suffix}");
    let (low, high) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    };
    emitter.blank();
    emitter.label_global(NAME);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, arg(emitter, 0), KEYS);
    abi::store_at_offset(emitter, arg(emitter, 1), VALUES);
    abi::store_at_offset(emitter, arg(emitter, 2), MODE);

    // -- validate the keys, then either the values array or the fill value --
    emit_unbox_array_slot(emitter, KEYS, &label("invalid_first"));
    abi::load_at_offset(emitter, result, MODE);
    emit_branch_if_result_equals(emitter, MODE_FILL, &label("fill_value"));
    emit_unbox_array_slot(emitter, VALUES, &label("invalid_second"));
    abi::load_at_offset(emitter, low, KEYS);
    abi::emit_load_from_address(emitter, low, low, 0);
    abi::load_at_offset(emitter, high, VALUES);
    abi::emit_load_from_address(emitter, high, high, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x1, x2");                                  // php requires as many values as keys
            emitter.instruction(&format!("b.ne {}", label("count_mismatch")));  // report the mismatch before allocating
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rdi, rdx");                                // php requires as many values as keys
            emitter.instruction(&format!("jne {}", label("count_mismatch")));   // report the mismatch before allocating
        }
    }
    abi::emit_store_zero_to_local_slot(emitter, FILL);
    abi::emit_jump(emitter, &label("operands_ready"));
    emitter.label(&label("fill_value"));
    abi::load_at_offset(emitter, result, VALUES);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::store_at_offset(emitter, result, FILL);
    abi::store_at_offset(emitter, low, FILL_LOW);
    abi::store_at_offset(emitter, high, FILL_HIGH);
    abi::emit_store_zero_to_local_slot(emitter, VALUES);
    // the fill value is copied once, so a handler that reassigns its variable cannot free it
    emit_own_cell(emitter, FILL, &label("fill_owned"));
    emitter.label(&label("fill_owned"));
    emitter.label(&label("operands_ready"));

    // -- keep both arrays alive while user code runs, then build under a cleanup boundary --
    abi::load_at_offset(emitter, result, KEYS);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, result, VALUES);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_store_zero_to_local_slot(emitter, RESULT);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    abi::emit_frame_slot_address(emitter, result, RESULT);
    exceptions::emit_guarded_cleanup_call(emitter, BODY, result, PENDING);
    abi::load_at_offset(emitter, result, KEYS);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, VALUES);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    emit_release_owned_cell(emitter, FILL, &label("fill_released"));
    emitter.label(&label("fill_released"));
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_nonzero(emitter, &label("throw"));
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    // -- a rendering threw: drop the partial result, then rethrow the chained exception --
    emitter.label(&label("throw"));
    abi::load_at_offset(emitter, result, RESULT);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_load_symbol_to_reg(emitter, low, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    // -- rejected operands: zero, with the position in the second result word --
    let position = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdx",
    };
    for (suffix, value) in [
        ("invalid_first", 1),
        ("invalid_second", 2),
        ("count_mismatch", COUNT_MISMATCH),
    ] {
        emitter.label(&label(suffix));
        abi::emit_load_int_immediate(emitter, position, value);
        abi::emit_jump(emitter, &label("invalid"));
    }
    emitter.label(&label("invalid"));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_body(emitter);
}

/// Fills the context's result hash, publishing the result pointer after every insert.
fn emit_body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg = |emitter: &Emitter, index: usize| abi::int_arg_reg_name(emitter.target, index);
    let label = |suffix: &str| format!("{BODY}_{suffix}");
    let (string, length) = abi::string_result_regs(emitter);
    let scratch = scratch_reg(emitter);
    emitter.blank();
    emitter.label_global(BODY);
    abi::emit_frame_prologue(emitter, BODY_FRAME);
    abi::store_at_offset(emitter, result, CONTEXT);

    // -- the result: as many slots as keys, per-entry tags under a Mixed summary --
    load_context_word(emitter, arg(emitter, 0), CTX_KEYS);
    abi::emit_load_from_address(emitter, arg(emitter, 0), arg(emitter, 0), 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, #8");                                  // never ask for a zero-capacity table
            emitter.instruction("cmp x0, x9");                                  // choose the larger capacity hint
            emitter.instruction("csel x0, x0, x9, ge");                         // size the result for every key
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 8");                                  // never ask for a zero-capacity table
            emitter.instruction("cmp rdi, rax");                                // choose the larger capacity hint
            emitter.instruction("cmovl rdi, rax");                              // size the result for every key
        }
    }
    abi::emit_load_int_immediate(emitter, arg(emitter, 1), 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
    publish(emitter, CTX_RESULT);
    abi::emit_store_zero_to_local_slot(emitter, KEY_CURSOR);
    abi::emit_store_zero_to_local_slot(emitter, VALUE_CURSOR);

    emitter.label(&label("loop"));
    emit_iter_next(emitter, CTX_KEYS, KEY_CURSOR, KEY, &label("done"));
    load_context_word(emitter, result, CTX_MODE);
    emit_branch_if_result_equals(emitter, MODE_FILL, &label("fill_value"));
    // the lengths were checked equal, so the values cannot run out first
    emit_iter_next(emitter, CTX_VALUES, VALUE_CURSOR, VALUE, &label("done"));
    abi::emit_jump(emitter, &label("value_ready"));
    emitter.label(&label("fill_value"));
    for (field, slot) in [(CTX_FILL, VALUE), (CTX_FILL + 8, VALUE_LOW), (CTX_FILL + 16, VALUE_HIGH)] {
        load_context_word(emitter, result, field);
        abi::store_at_offset_scratch(emitter, result, slot, scratch);
    }
    emitter.label(&label("value_ready"));

    // -- the key: integers stand, everything else renders and normalizes --
    let (concat_addr, concat_value) = concat_regs(emitter);
    abi::emit_symbol_address(emitter, concat_addr, "_concat_off");
    abi::emit_load_from_address(emitter, concat_value, concat_addr, 0);
    abi::store_at_offset(emitter, concat_value, CONCAT);
    abi::emit_store_zero_to_local_slot(emitter, RENDER);
    abi::load_at_offset(emitter, result, KEY);
    abi::emit_branch_if_int_result_zero(emitter, &label("int_key"));
    emit_branch_if_result_equals(emitter, 1, &label("string_key"));
    abi::emit_frame_slot_address(emitter, result, KEY);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    abi::store_at_offset_scratch(emitter, string, RENDER, scratch);
    abi::emit_jump(emitter, &label("normalize"));
    emitter.label(&label("string_key"));
    abi::load_at_offset(emitter, string, KEY_LOW);
    abi::load_at_offset(emitter, length, KEY_HIGH);
    emitter.label(&label("normalize"));
    abi::emit_call_label(emitter, "__rt_hash_normalize_key");
    let (key_lo, key_hi) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rax", "rdx"),
    };
    abi::store_at_offset_scratch(emitter, key_lo, NORMAL_LO, scratch);
    abi::store_at_offset_scratch(emitter, key_hi, NORMAL_HI, scratch);
    abi::emit_jump(emitter, &label("key_ready"));
    emitter.label(&label("int_key"));
    abi::load_at_offset(emitter, result, KEY_LOW);
    abi::store_at_offset(emitter, result, NORMAL_LO);
    abi::emit_load_int_immediate(emitter, result, -1);
    abi::store_at_offset(emitter, result, NORMAL_HI);
    emitter.label(&label("key_ready"));

    // -- insert a copy the result owns, then drop the rendering --
    emit_own_cell(emitter, VALUE, &label("value_owned"));
    emitter.label(&label("value_owned"));
    load_context_word(emitter, arg(emitter, 0), CTX_RESULT);
    abi::load_at_offset(emitter, arg(emitter, 1), NORMAL_LO);
    abi::load_at_offset(emitter, arg(emitter, 2), NORMAL_HI);
    abi::load_at_offset(emitter, arg(emitter, 3), VALUE_LOW);
    abi::load_at_offset(emitter, arg(emitter, 4), VALUE_HIGH);
    abi::load_at_offset(emitter, arg(emitter, 5), VALUE);
    abi::emit_call_label(emitter, "__rt_hash_set");
    publish(emitter, CTX_RESULT);
    abi::load_at_offset(emitter, result, RENDER);
    abi::emit_call_label(emitter, "__rt_heap_free");
    let (concat_addr, concat_value) = concat_regs(emitter);
    abi::emit_symbol_address(emitter, concat_addr, "_concat_off");
    abi::load_at_offset(emitter, concat_value, CONCAT);
    abi::emit_store_to_address(emitter, concat_value, concat_addr, 0);
    abi::emit_jump(emitter, &label("loop"));

    emitter.label(&label("done"));
    abi::emit_frame_restore(emitter, BODY_FRAME);
    abi::emit_return(emitter);
}

/// Loads one word of the boundary's context into `dest`.
fn load_context_word(emitter: &mut Emitter, dest: &str, field: usize) {
    abi::load_at_offset(emitter, dest, CONTEXT);
    abi::emit_load_from_address(emitter, dest, dest, field);
}

/// Stores the integer result register into one word of the boundary's context.
fn publish(emitter: &mut Emitter, field: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, CONTEXT);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), scratch, field);
}

/// Unboxes the borrowed cell whose pointer is in `slot`, rejects a non-array, and replaces the
/// slot with the array payload pointer.
fn emit_unbox_array_slot(emitter: &mut Emitter, slot: usize, invalid: &str) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, slot);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub x9, x0, #4");                              // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp x9, #1");                                  // only PHP arrays may be traversed
            emitter.instruction(&format!("b.hi {invalid}"));                    // reject scalar, object and null payloads before dereference
            abi::store_at_offset_scratch(emitter, "x1", slot, "x10");
        }
        Arch::X86_64 => {
            emitter.instruction("lea r10, [rax - 4]");                          // fold valid packed/hash tags into zero and one
            emitter.instruction("cmp r10, 1");                                  // only PHP arrays may be traversed
            emitter.instruction(&format!("ja {invalid}"));                      // reject scalar, object and null payloads before dereference
            abi::store_at_offset(emitter, "rdi", slot);
        }
    }
}

/// Advances the iterator over the array in context word `field` with the cursor in
/// `cursor_slot`, branching to `done` when exhausted, and spills the element cell at `cell`.
fn emit_iter_next(emitter: &mut Emitter, field: usize, cursor_slot: usize, cell: usize, done: &str) {
    let result = abi::int_result_reg(emitter);
    load_context_word(emitter, abi::int_arg_reg_name(emitter.target, 0), field);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), cursor_slot);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    let (tag, lo, hi) = match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // minus one signals iterator exhaustion
            emitter.instruction(&format!("b.eq {done}"));                       // the whole array has been visited
            ("x3", "x4", "x5")
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // minus one signals iterator exhaustion
            emitter.instruction(&format!("je {done}"));                         // the whole array has been visited
            ("r8", "r9", "r10")
        }
    };
    let scratch = scratch_reg(emitter);
    for (reg, slot) in [(result, cursor_slot), (tag, cell), (lo, cell - 8), (hi, cell - 16)] {
        abi::store_at_offset_scratch(emitter, reg, slot, scratch);
    }
}

/// Gives the cell at `cell` (tag, then low and high words at `cell - 8` / `cell - 16`) a copy
/// its new owner holds: a string is duplicated with `__rt_str_persist`, the heap-backed tags
/// 4..=7, callables (10) and reference cells (11) are retained. Falls through to `owned`.
fn emit_own_cell(emitter: &mut Emitter, cell: usize, owned: &str) {
    let result = abi::int_result_reg(emitter);
    let retain = format!("{owned}_retain");
    let persist = format!("{owned}_persist");
    let tag = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, tag, cell);
    emit_tag_ladder(emitter, tag, &persist, &retain, owned);
    emitter.label(&retain);
    abi::load_at_offset(emitter, result, cell - 8);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, owned);
    emitter.label(&persist);
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, ptr, cell - 8);
    abi::load_at_offset(emitter, len, cell - 16);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset_scratch(emitter, ptr, cell - 8, scratch_reg(emitter));
    abi::store_at_offset_scratch(emitter, len, cell - 16, scratch_reg(emitter));
}

/// Releases the copy `emit_own_cell` made of the cell at `cell`. Falls through to `released`.
fn emit_release_owned_cell(emitter: &mut Emitter, cell: usize, released: &str) {
    let result = abi::int_result_reg(emitter);
    let retain = format!("{released}_decref");
    let persist = format!("{released}_free");
    let tag = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, tag, cell);
    emit_tag_ladder(emitter, tag, &persist, &retain, released);
    emitter.label(&retain);
    abi::load_at_offset(emitter, result, cell - 8);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::emit_jump(emitter, released);
    emitter.label(&persist);
    abi::load_at_offset(emitter, result, cell - 8);
    abi::emit_call_label(emitter, "__rt_heap_free");
}

/// Branches on a runtime value tag: strings to `string`, reference-counted payloads to `heap`,
/// everything else to `plain`.
fn emit_tag_ladder(emitter: &mut Emitter, tag: &str, string: &str, heap: &str, plain: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #1"));                     // a string payload is owned by its array
            emitter.instruction(&format!("b.eq {string}"));                     // strings are copied, not shared
            emitter.instruction(&format!("cmp {tag}, #4"));                     // tags below 4 are scalar payloads
            emitter.instruction(&format!("b.lt {plain}"));                      // scalars own nothing
            emitter.instruction(&format!("cmp {tag}, #7"));                     // arrays, hashes, objects and boxes are heap-backed
            emitter.instruction(&format!("b.le {heap}"));                       // these carry a reference count
            emitter.instruction(&format!("cmp {tag}, #10"));                    // callables and reference cells are heap-backed too
            emitter.instruction(&format!("b.lt {plain}"));                      // null and resources own nothing
            emitter.instruction(&format!("b {heap}"));                          // callables and reference cells carry a reference count
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, 1"));                      // a string payload is owned by its array
            emitter.instruction(&format!("je {string}"));                       // strings are copied, not shared
            emitter.instruction(&format!("cmp {tag}, 4"));                      // tags below 4 are scalar payloads
            emitter.instruction(&format!("jl {plain}"));                        // scalars own nothing
            emitter.instruction(&format!("cmp {tag}, 7"));                      // arrays, hashes, objects and boxes are heap-backed
            emitter.instruction(&format!("jle {heap}"));                        // these carry a reference count
            emitter.instruction(&format!("cmp {tag}, 10"));                     // callables and reference cells are heap-backed too
            emitter.instruction(&format!("jl {plain}"));                        // null and resources own nothing
            emitter.instruction(&format!("jmp {heap}"));                        // callables and reference cells carry a reference count
        }
    }
}

/// Branches when the integer result register equals `value`.
fn emit_branch_if_result_equals(emitter: &mut Emitter, value: i64, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{value}"));                  // compare the loaded word with the constant
            emitter.instruction(&format!("b.eq {target}"));                     // take the branch on equality
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {value}"));                  // compare the loaded word with the constant
            emitter.instruction(&format!("je {target}"));                       // take the branch on equality
        }
    }
}

/// A scratch register for far frame-slot stores, distinct from every value being stored.
fn scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r11",
    }
}

/// Two registers that hold the `_concat_off` address and value while it is saved or restored.
fn concat_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x10", "x11"),
        Arch::X86_64 => ("r11", "r10"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The builder walks both layouts, renders non-integer keys, normalizes them, and runs under
    /// a cleanup boundary that retains its operands first, on every target.
    #[test]
    fn combine_builder_renders_keys_under_a_cleanup_boundary_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_combine_boxed(&mut emitter);
            let asm = emitter.output();
            let (boundary, body) = asm.split_once(&format!("{BODY}:")).unwrap();
            assert!(boundary.contains("__rt_cleanup_invoke"), "{target}");
            assert!(boundary.contains("__rt_throw_current"), "{target}");
            assert!(
                boundary.find("__rt_incref").unwrap() < boundary.find("__rt_cleanup_invoke").unwrap(),
                "{target}: retain the operands before user code can run"
            );
            for helper in [
                "__rt_array_iter_next",
                "__rt_mixed_cast_string",
                "__rt_hash_normalize_key",
                "__rt_hash_set",
                "__rt_heap_free",
            ] {
                assert!(body.contains(helper), "{target}: {helper}");
            }
        }
    }
}
