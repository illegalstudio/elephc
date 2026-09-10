//! Purpose:
//! Searches and filters boxed PHP arrays through storage-neutral value/key predicates.
//!
//! Called from:
//! - The ArrayFind, ArrayAny, ArrayAll and ArrayFilter backend paths.
//!
//! Key details:
//! - Consumes a descriptor and borrows a validated source triple.
//! - A separate payload snapshot keeps iteration stable across callback mutation.
//! - Every temporary has an owner slot covered by a resumable native exception boundary.

use crate::codegen_support::{abi, arrays, emit::Emitter, platform::Arch, value_boxing};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::types::PhpType;

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 192;
const HANDLER: usize = FRAME - 16;
const CALLBACK: usize = 8;
const BORROWED_SOURCE: usize = 16;
const MODE: usize = 24;
const SOURCE: usize = 32;
const PAYLOAD: usize = 40;
const CURSOR: usize = 48;
const INPUT: usize = 56;
const KEY: usize = 64;
const ARGUMENTS: usize = 72;
const NEXT: usize = 80;
const ANSWER: usize = 88;
const TRUTH: usize = 96;
const ENTRY: usize = 120;
const KEY_LO: usize = 128;
const KEY_HI: usize = 136;
const PREVIOUS: usize = 144;
const PENDING: usize = 152;

/// Borrows a source and consumes a descriptor: modes 0/1/2 search, modes 3/4/5 filter value/both/key.
/// A null descriptor selects callback-free filtering; every mode returns an owned Mixed cell.
pub fn emit_array_predicate_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_predicate_boxed");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CALLBACK, BORROWED_SOURCE, MODE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [SOURCE, CURSOR, INPUT, KEY, ARGUMENTS, NEXT, ANSWER, PENDING] {
        clear_slot(emitter, offset);
    }
    install_boundary(emitter);
    abi::load_at_offset(emitter, result, BORROWED_SOURCE);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::store_at_offset(emitter, value_low_reg(emitter), PAYLOAD);
    box_unboxed_value(emitter);
    abi::store_at_offset(emitter, result, SOURCE);
    initialize_answer(emitter);

    // -- the iterator supplies logical keys and complete value triples for either layout --
    emitter.label("__rt_array_predicate_boxed_loop");
    for (index, offset) in [PAYLOAD, CURSOR].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    ins(emitter, "b.eq __rt_array_predicate_boxed_cleanup", "je __rt_array_predicate_boxed_cleanup");
    abi::store_at_offset(emitter, result, CURSOR);
    let fields = if emitter.target.arch == Arch::AArch64 {
        ["x1", "x2", "x3", "x4", "x5"]
    } else { ["rcx", "rdx", "r8", "r9", "r10"] };
    for (reg, offset) in fields.into_iter().zip([KEY_LO, KEY_HI, ENTRY, ENTRY - 8, ENTRY - 16]) {
        abi::store_at_offset(emitter, reg, offset);
    }
    abi::emit_frame_slot_address(emitter, result, ENTRY);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    box_unboxed_value(emitter);
    abi::store_at_offset(emitter, result, INPUT);
    box_key(emitter);
    abi::store_at_offset(emitter, result, KEY);
    abi::load_at_offset(emitter, result, CALLBACK);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_predicate_boxed_no_callback");
    prepare_arguments(emitter);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CALLBACK);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), ARGUMENTS);
    clear_slot(emitter, ARGUMENTS);
    abi::emit_call_label(emitter, "__rt_callable_invoke_owned_args");
    abi::store_at_offset(emitter, result, NEXT);
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
    abi::store_at_offset(emitter, result, TRUTH);
    release_slot(emitter, NEXT, "__rt_decref_mixed");
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_decide");
    emitter.label("__rt_array_predicate_boxed_no_callback");
    abi::load_at_offset(emitter, result, INPUT);
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
    abi::store_at_offset(emitter, result, TRUTH);
    emitter.label("__rt_array_predicate_boxed_decide");
    decide_match(emitter);

    emitter.label("__rt_array_predicate_boxed_next");
    release_slot(emitter, INPUT, "__rt_decref_mixed");
    release_slot(emitter, KEY, "__rt_decref_mixed");
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_loop");

    // -- clear owners before release, including when a destructor interrupts cleanup --
    emitter.label("__rt_array_predicate_boxed_cleanup");
    for (offset, helper) in [
        (INPUT, "__rt_decref_mixed"), (KEY, "__rt_decref_mixed"),
        (ARGUMENTS, "__rt_decref_any"), (NEXT, "__rt_decref_mixed"),
    ] { release_slot(emitter, offset, helper); }
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_predicate_boxed_release_source");
    release_slot(emitter, ANSWER, "__rt_decref_mixed");
    emitter.label("__rt_array_predicate_boxed_release_source");
    release_slot(emitter, SOURCE, "__rt_decref_mixed");
    release_slot(emitter, CALLBACK, "__rt_callable_descriptor_release");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_predicate_boxed_return");
    abi::load_at_offset(emitter, value_low_reg(emitter), PREVIOUS);
    clear_slot(emitter, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label("__rt_array_predicate_boxed_return");
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::load_at_offset(emitter, result, ANSWER);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    emitter.label("__rt_array_predicate_boxed_caught");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, value_low_reg(emitter), PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_cleanup");
}

/// Creates null/false/true for searches, or an independent keyed result for filtering.
fn initialize_answer(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, MODE);
    ins(emitter, "cmp x0, #3", "cmp rax, 3");
    ins(emitter, "b.ge __rt_array_predicate_boxed_filter_answer", "jge __rt_array_predicate_boxed_filter_answer");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_predicate_boxed_null_answer");
    ins(emitter, "cmp x0, #2", "cmp rax, 2");
    ins(emitter, "cset x1, eq", "sete dil");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("movzx edi, dil");                                  // widen the default boolean payload after testing all mode
    }
    abi::emit_load_int_immediate(emitter, result, 3);
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_box_answer");
    emitter.label("__rt_array_predicate_boxed_null_answer");
    abi::emit_load_int_immediate(emitter, result, 8);
    abi::emit_load_int_immediate(emitter, value_low_reg(emitter), 0);
    emitter.label("__rt_array_predicate_boxed_box_answer");
    abi::emit_load_int_immediate(emitter, value_high_reg(emitter), 0);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, ANSWER);
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_answer_ready");
    emitter.label("__rt_array_predicate_boxed_filter_answer");
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 4);
    abi::emit_call_label(emitter, "__rt_hash_new");
    value_boxing::emit_box_current_owned_value_as_mixed(emitter, &PhpType::AssocArray {
        key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed),
    });
    abi::store_at_offset(emitter, result, ANSWER);
    emitter.label("__rt_array_predicate_boxed_answer_ready");
}

/// Preserves integer keys and copies string keys into an independently owned callback argument.
fn box_key(emitter: &mut Emitter) {
    abi::load_at_offset(emitter, value_low_reg(emitter), KEY_LO);
    abi::load_at_offset(emitter, value_high_reg(emitter), KEY_HI);
    ins(emitter, "cmp x2, #0", "cmp rsi, 0");
    ins(emitter, "cset x0, ge", "setge al");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("movzx eax, al");                                   // integer keys use tag zero and string keys use tag one
    }
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_predicate_boxed_string_key");
    abi::emit_load_int_immediate(emitter, value_high_reg(emitter), 0);
    emitter.label("__rt_array_predicate_boxed_string_key");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Builds owned callback arguments while retaining the original value/key for result publication.
fn prepare_arguments(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, arg0, 2);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    arrays::emit_array_value_type_stamp(emitter, result, &PhpType::Mixed);
    abi::store_at_offset(emitter, result, ARGUMENTS);
    abi::load_at_offset(emitter, result, MODE);
    ins(emitter, "cmp x0, #5", "cmp rax, 5");
    ins(emitter, "b.eq __rt_array_predicate_boxed_push_key", "je __rt_array_predicate_boxed_push_key");
    abi::load_at_offset(emitter, result, INPUT);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, arg1, INPUT);
    abi::load_at_offset(emitter, arg0, ARGUMENTS);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    abi::store_at_offset(emitter, result, ARGUMENTS);
    abi::load_at_offset(emitter, result, MODE);
    ins(emitter, "cmp x0, #3", "cmp rax, 3");
    ins(emitter, "b.eq __rt_array_predicate_boxed_box_arguments", "je __rt_array_predicate_boxed_box_arguments");
    emitter.label("__rt_array_predicate_boxed_push_key");
    abi::load_at_offset(emitter, result, KEY);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, arg1, KEY);
    abi::load_at_offset(emitter, arg0, ARGUMENTS);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    abi::store_at_offset(emitter, result, ARGUMENTS);
    emitter.label("__rt_array_predicate_boxed_box_arguments");
    abi::load_at_offset(emitter, result, ARGUMENTS);
    value_boxing::emit_box_current_owned_value_as_mixed(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::store_at_offset(emitter, result, ARGUMENTS);
}

/// Stops a search at its first decisive result, or preserves a kept filter entry's original key.
fn decide_match(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, MODE);
    ins(emitter, "cmp x0, #3", "cmp rax, 3");
    ins(emitter, "b.ge __rt_array_predicate_boxed_filter_match", "jge __rt_array_predicate_boxed_filter_match");
    ins(emitter, "cmp x0, #2", "cmp rax, 2");
    ins(emitter, "b.eq __rt_array_predicate_boxed_all", "je __rt_array_predicate_boxed_all");
    abi::load_at_offset(emitter, result, TRUTH);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_predicate_boxed_next");
    abi::load_at_offset(emitter, result, MODE);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_predicate_boxed_boolean_match");
    release_slot(emitter, ANSWER, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, INPUT);
    clear_slot(emitter, INPUT);
    abi::store_at_offset(emitter, result, ANSWER);
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_cleanup");
    emitter.label("__rt_array_predicate_boxed_all");
    abi::load_at_offset(emitter, result, TRUTH);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_predicate_boxed_next");
    emitter.label("__rt_array_predicate_boxed_boolean_match");
    release_slot(emitter, ANSWER, "__rt_decref_mixed");
    abi::load_at_offset(emitter, value_low_reg(emitter), TRUTH);
    abi::emit_load_int_immediate(emitter, value_high_reg(emitter), 0);
    abi::emit_load_int_immediate(emitter, result, 3);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, ANSWER);
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_cleanup");
    emitter.label("__rt_array_predicate_boxed_filter_match");
    abi::load_at_offset(emitter, result, TRUTH);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_predicate_boxed_next");
    for (index, offset) in [ANSWER, KEY_LO, KEY_HI, INPUT].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    clear_slot(emitter, INPUT);
    abi::emit_call_label(emitter, "__rt_mixed_array_set");
    abi::emit_jump(emitter, "__rt_array_predicate_boxed_next");
}

/// Adapts the unboxed high word to the retaining Mixed constructor's legacy x86_64 ABI.
fn box_unboxed_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rsi, rdx");                                    // preserve strings and paired values while acquiring a fresh cell
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Installs a native handler before any source, result or callback argument owner is acquired.
fn install_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_value", PREVIOUS), ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                    // keep all predicate-owned cells reachable across callback and cleanup throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_predicate_boxed_caught");
}

/// Restores the caller's handler and diagnostic depth after predicate ownership cleanup.
fn restore_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
}

/// Clears a slot without overwriting the current owner or callback ABI arguments.
fn clear_slot(emitter: &mut Emitter, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset);
}

/// Retires an owner after clearing its slot so interrupted cleanup can resume safely.
fn release_slot(emitter: &mut Emitter, offset: usize, helper: &str) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::emit_call_label(emitter, helper);
}

/// Returns the Mixed payload's low-word register and exception-chain older-owner argument.
fn value_low_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" }
}

/// Returns the high-word register consumed by the retaining Mixed constructor.
fn value_high_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }
}

/// Emits equivalent predicate control flow for either supported native architecture.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // preserve storage-neutral predicate decisions across targets
}

#[cfg(test)]
mod tests {
    //! Verifies predicate snapshot ownership and cleanup ordering across all supported targets.

    use super::*;
    use crate::codegen_support::platform::Target;

    /// The predicate iterator and both argument owners are protected before descriptor invocation.
    #[test]
    fn boxed_predicate_ownership_boundary_covers_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_predicate_boxed(&mut emitter);
            let asm = emitter.output();
            let handler = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let snapshot = asm.find("__rt_mixed_from_value").unwrap();
            let iterator = asm.find("__rt_array_iter_next").unwrap();
            let callback = asm.find("__rt_callable_invoke_owned_args").unwrap();
            assert!(handler < snapshot && snapshot < iterator && iterator < callback, "{name}");
            assert!(asm.contains("__rt_mixed_cast_bool"), "{name}");
            assert!(asm.contains("__rt_array_predicate_boxed_no_callback:"), "{name}");
            assert!(asm.contains("__rt_array_predicate_boxed_filter_match:"), "{name}");
            assert!(asm.contains("__rt_mixed_array_set"), "{name}: filters publish original keys and owned cells");
            assert!(asm.contains("__rt_array_predicate_boxed_caught:"), "{name}");
            assert!(asm.contains("__rt_callable_descriptor_release"), "{name}");
            assert!(asm.contains("__rt_exception_chain"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
            assert!(!asm.contains("__rt_mixed_clone"), "{name}: borrowed triples are not heap cells");
        }
    }
}
