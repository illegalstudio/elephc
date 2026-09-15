//! Purpose:
//! Flips boxed PHP arrays using logical keys and runtime value tags.
//!
//! Called from:
//! - The typed ArrayFlip lowering for declared PHP array storage.
//!
//! Key details:
//! - Retains the source payload across warning handlers and their mutations.
//! - Owns the partial result across throws, then returns an independent hash.
//! - Values other than integers and strings produce a warning and are skipped.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::exceptions};

const FRAME: usize = 48;
const SOURCE: usize = 8;
const OUTPUT: usize = 16;
const PENDING: usize = 24;
const BODY_FRAME: usize = 96;
const CONTEXT: usize = 8;
const CURSOR: usize = 16;
const KEY_LO: usize = 24;
const KEY_HI: usize = 32;
const VALUE_HI: usize = 40;
const VALUE_LO: usize = 48;
const VALUE_TAG: usize = 56;
const FLIPPED_LO: usize = 64;
const FLIPPED_HI: usize = 72;

/// Selects the target instruction for one shared runtime operation.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // preserve the shared operation across both native ABIs
}

/// Borrows a boxed C argument and returns an owned hash, or zero for a non-array.
pub fn emit_array_flip_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let low = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    emitter.blank();
    emitter.label_global("__rt_array_flip_boxed");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    ins(emitter, "sub x9, x0, #4", "lea r10, [rax - 4]");
    ins(emitter, "cmp x9, #1", "cmp r10, 1");
    ins(emitter, "b.hi __rt_array_flip_boxed_invalid", "ja __rt_array_flip_boxed_invalid");
    abi::store_at_offset(emitter, low, SOURCE);
    abi::emit_reg_move(emitter, result, low);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_store_zero_to_local_slot(emitter, OUTPUT);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    abi::emit_frame_slot_address(emitter, result, OUTPUT);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_array_flip_boxed_body", result, PENDING);
    abi::load_at_offset(emitter, result, SOURCE);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_flip_boxed_throw");
    abi::load_at_offset(emitter, result, OUTPUT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    emitter.label("__rt_array_flip_boxed_throw");
    abi::load_at_offset(emitter, result, OUTPUT);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_load_symbol_to_reg(emitter, low, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label("__rt_array_flip_boxed_invalid");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_body(emitter);
}

/// Populates the boundary's output slot while borrowing its retained source snapshot.
fn emit_body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let (low, high) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rdx") };
    emitter.blank();
    emitter.label_global("__rt_array_flip_boxed_body");
    abi::emit_frame_prologue(emitter, BODY_FRAME);
    abi::store_at_offset(emitter, result, CONTEXT);
    abi::emit_load_int_immediate(emitter, arg0, 16);
    abi::emit_load_int_immediate(emitter, arg1, 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
    store_output(emitter);
    abi::emit_store_zero_to_local_slot(emitter, CURSOR);
    emitter.label("__rt_array_flip_boxed_loop");
    abi::load_at_offset(emitter, arg0, CONTEXT);
    abi::emit_load_from_address(emitter, arg0, arg0, 8);
    abi::load_at_offset(emitter, arg1, CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    ins(emitter, "b.eq __rt_array_flip_boxed_done", "je __rt_array_flip_boxed_done");
    abi::store_at_offset(emitter, result, CURSOR);
    let fields = if emitter.target.arch == Arch::AArch64 {
        ["x1", "x2", "x3", "x4", "x5"]
    } else {
        ["rcx", "rdx", "r8", "r9", "r10"]
    };
    for (reg, offset) in fields.into_iter().zip([KEY_LO, KEY_HI, VALUE_TAG, VALUE_LO, VALUE_HI]) {
        abi::store_at_offset(emitter, reg, offset);
    }
    abi::emit_frame_slot_address(emitter, result, VALUE_TAG);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::store_at_offset(emitter, low, VALUE_LO);
    abi::store_at_offset(emitter, high, VALUE_HI);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_flip_boxed_int");
    ins(emitter, "cmp x0, #1", "cmp rax, 1");
    ins(emitter, "b.ne __rt_array_flip_boxed_skip", "jne __rt_array_flip_boxed_skip");

    // String values become normalized string or integer destination keys.
    let (string, _) = abi::string_result_regs(emitter);
    abi::emit_reg_move(emitter, string, low);
    abi::emit_call_label(emitter, "__rt_hash_normalize_key");
    let (key, key_length) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rax", "rdx") };
    abi::store_at_offset(emitter, key, FLIPPED_LO);
    abi::store_at_offset(emitter, key_length, FLIPPED_HI);
    abi::emit_jump(emitter, "__rt_array_flip_boxed_value");
    emitter.label("__rt_array_flip_boxed_int");
    abi::store_at_offset(emitter, low, FLIPPED_LO);
    abi::emit_load_int_immediate(emitter, scratch, -1);
    abi::store_at_offset(emitter, scratch, FLIPPED_HI);

    emitter.label("__rt_array_flip_boxed_value");
    let (string, length) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, string, KEY_LO);
    abi::load_at_offset(emitter, length, KEY_HI);
    ins(emitter, "cmn x2, #1", "cmp rdx, -1");
    ins(emitter, "b.eq __rt_array_flip_boxed_value_int", "je __rt_array_flip_boxed_value_int");
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset(emitter, string, VALUE_LO);
    abi::store_at_offset(emitter, length, VALUE_HI);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, "__rt_array_flip_boxed_insert");
    emitter.label("__rt_array_flip_boxed_value_int");
    abi::store_at_offset(emitter, string, VALUE_LO);
    abi::emit_store_zero_to_local_slot(emitter, VALUE_HI);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label("__rt_array_flip_boxed_insert");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 5), result);
    for (index, offset) in [(1, FLIPPED_LO), (2, FLIPPED_HI), (3, VALUE_LO), (4, VALUE_HI)] {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, arg0, CONTEXT);
    abi::emit_load_from_address(emitter, arg0, arg0, 0);
    abi::emit_call_label(emitter, "__rt_hash_set");
    store_output(emitter);
    abi::emit_jump(emitter, "__rt_array_flip_boxed_loop");

    emitter.label("__rt_array_flip_boxed_skip");
    let (symbol, message) = super::ARRAY_FLIP_SKIPPED_MESSAGES[0];
    let (warning, warning_len) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rsi") };
    abi::emit_symbol_address(emitter, warning, symbol);
    abi::emit_load_int_immediate(emitter, warning_len, message.len() as i64);
    abi::emit_call_label(emitter, "__rt_diag_warning");
    abi::emit_jump(emitter, "__rt_array_flip_boxed_loop");
    emitter.label("__rt_array_flip_boxed_done");
    abi::emit_frame_restore(emitter, BODY_FRAME);
    abi::emit_return(emitter);
}

/// Publishes the current hash owner before another warning can enter user code.
fn store_output(emitter: &mut Emitter) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, CONTEXT);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), scratch, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Pins logical iteration and bounded source/result cleanup on every native target.
    #[test]
    fn boxed_flip_guards_warning_callbacks_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_flip_boxed(&mut emitter);
            let asm = emitter.output();
            let (boundary, body) = asm.split_once("__rt_array_flip_boxed_body:").unwrap();
            assert_eq!(boundary.matches("__rt_cleanup_invoke").count(), 3, "{name}");
            assert!(boundary.find("__rt_incref").unwrap() < boundary.find("__rt_cleanup_invoke").unwrap(), "{name}");
            assert!(boundary.contains("__rt_throw_current"), "{name}");
            for helper in ["__rt_array_iter_next", "__rt_mixed_unbox", "__rt_hash_normalize_key", "__rt_diag_warning"] {
                assert!(body.contains(helper), "{name}: {helper}");
            }
        }
    }
}
