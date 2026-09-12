//! Purpose:
//! Selects boxed PHP-array entries through a two-value comparator.
//!
//! Called from:
//! - The ArrayUdiff and ArrayUintersect backend paths.
//!
//! Key details:
//! - Consumes a descriptor and borrows two validated array triples.
//! - Independent payload snapshots preserve iteration across callback mutation.
//! - The O(n*m) scan preserves first-array keys, duplicate values and value ownership.
//! - A resumable native exception boundary owns all intermediate cells.

use crate::codegen_support::{abi, arrays, emit::Emitter, platform::Arch, value_boxing};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::types::PhpType;

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 224;
const HANDLER: usize = FRAME - 16;
const CALLBACK: usize = 8;
const BORROWED_FIRST: usize = 16;
const BORROWED_SECOND: usize = 24;
const MODE: usize = 32;
const FIRST: usize = 40;
const SECOND: usize = 48;
const FIRST_PAYLOAD: usize = 56;
const SECOND_PAYLOAD: usize = 64;
const INPUT: usize = 72;
const KEY: usize = 80;
const ARGUMENTS: usize = 88;
const NEXT: usize = 96;
const ANSWER: usize = 104;
const FIRST_CURSOR: usize = 112;
const SECOND_CURSOR: usize = 120;
const ENTRY: usize = 144;
const KEY_LO: usize = 152;
const KEY_HI: usize = 160;
const COMPARISON: usize = 168;
const PREVIOUS: usize = 176;
const PENDING: usize = 184;
const OTHER: usize = 192;

/// Takes descriptor/first/second/mode in C arguments and returns an owned boxed keyed array.
/// Mode zero keeps unmatched entries; mode one keeps entries with any comparator-equal value.
pub fn emit_array_udiff_uintersect(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_udiff_uintersect");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CALLBACK, BORROWED_FIRST, BORROWED_SECOND, MODE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [FIRST, SECOND, INPUT, OTHER, KEY, ARGUMENTS, NEXT, ANSWER, FIRST_CURSOR, PENDING] {
        clear_slot(emitter, offset);
    }
    install_boundary(emitter);
    for (borrowed, payload, owner) in [
        (BORROWED_FIRST, FIRST_PAYLOAD, FIRST), (BORROWED_SECOND, SECOND_PAYLOAD, SECOND),
    ] {
        abi::load_at_offset(emitter, result, borrowed);
        abi::emit_call_label(emitter, "__rt_mixed_unbox");
        abi::store_at_offset(emitter, value_low_reg(emitter), payload);
        box_unboxed_value(emitter);
        abi::store_at_offset(emitter, result, owner);
    }
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 4);
    abi::emit_call_label(emitter, "__rt_hash_new");
    value_boxing::emit_box_current_owned_value_as_mixed(emitter, &PhpType::AssocArray {
        key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed),
    });
    abi::store_at_offset(emitter, result, ANSWER);

    // -- retain each candidate and its logical key independently of either source layout --
    emitter.label("__rt_array_udiff_uintersect_outer");
    iterate(emitter, FIRST_PAYLOAD, FIRST_CURSOR, "__rt_array_udiff_uintersect_cleanup", true);
    box_entry(emitter, INPUT);
    box_key(emitter);
    abi::store_at_offset(emitter, result, KEY);
    clear_slot(emitter, SECOND_CURSOR);

    emitter.label("__rt_array_udiff_uintersect_inner");
    iterate(emitter, SECOND_PAYLOAD, SECOND_CURSOR, "__rt_array_udiff_uintersect_absent", false);
    box_entry(emitter, OTHER);
    prepare_arguments(emitter);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CALLBACK);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), ARGUMENTS);
    clear_slot(emitter, ARGUMENTS);
    abi::emit_call_label(emitter, "__rt_callable_invoke_owned_args");
    abi::store_at_offset(emitter, result, NEXT);
    abi::emit_call_label(emitter, "__rt_mixed_cast_int");
    abi::store_at_offset(emitter, result, COMPARISON);
    release_slot(emitter, NEXT, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, COMPARISON);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_udiff_uintersect_inner");
    abi::load_at_offset(emitter, result, MODE);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_udiff_uintersect_advance");
    abi::emit_jump(emitter, "__rt_array_udiff_uintersect_keep");

    emitter.label("__rt_array_udiff_uintersect_absent");
    abi::load_at_offset(emitter, result, MODE);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_udiff_uintersect_advance");
    emitter.label("__rt_array_udiff_uintersect_keep");
    for (index, offset) in [ANSWER, KEY_LO, KEY_HI, INPUT].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    clear_slot(emitter, INPUT);
    abi::emit_call_label(emitter, "__rt_mixed_array_set");

    emitter.label("__rt_array_udiff_uintersect_advance");
    release_slot(emitter, INPUT, "__rt_decref_mixed");
    release_slot(emitter, KEY, "__rt_decref_mixed");
    abi::emit_jump(emitter, "__rt_array_udiff_uintersect_outer");
    emit_cleanup(emitter);
}

/// Advances an array iterator, saving the complete value triple and optionally its original key.
fn iterate(emitter: &mut Emitter, payload: usize, cursor: usize, done: &str, preserve_key: bool) {
    for (index, offset) in [payload, cursor].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    ins(emitter, &format!("b.eq {done}"), &format!("je {done}"));
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), cursor);
    let (keys, values) = if emitter.target.arch == Arch::AArch64 {
        (["x1", "x2"], ["x3", "x4", "x5"])
    } else { (["rcx", "rdx"], ["r8", "r9", "r10"]) };
    if preserve_key {
        for (reg, offset) in keys.into_iter().zip([KEY_LO, KEY_HI]) {
            abi::store_at_offset(emitter, reg, offset);
        }
    }
    for (reg, offset) in values.into_iter().zip([ENTRY, ENTRY - 8, ENTRY - 16]) {
        abi::store_at_offset(emitter, reg, offset);
    }
}

/// Acquires an iterator entry's payload in a fresh Mixed owner, including nested arrays and objects.
fn box_entry(emitter: &mut Emitter, owner: usize) {
    abi::emit_frame_slot_address(emitter, abi::int_result_reg(emitter), ENTRY);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    box_unboxed_value(emitter);
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), owner);
}

/// Keeps string key bytes alive while comparator callbacks mutate or retire original sources.
fn box_key(emitter: &mut Emitter) {
    abi::load_at_offset(emitter, value_low_reg(emitter), KEY_LO);
    abi::load_at_offset(emitter, value_high_reg(emitter), KEY_HI);
    ins(emitter, "cmp x2, #0", "cmp rsi, 0");
    ins(emitter, "cset x0, ge", "setge al");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("movzx eax, al");                                   // select integer or string key tag without stale upper bits
    }
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_udiff_uintersect_string_key");
    abi::emit_load_int_immediate(emitter, value_high_reg(emitter), 0);
    emitter.label("__rt_array_udiff_uintersect_string_key");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Builds a two-cell argument array, retaining the left input and transferring the right candidate.
fn prepare_arguments(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, arg0, 2);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    arrays::emit_array_value_type_stamp(emitter, result, &PhpType::Mixed);
    abi::store_at_offset(emitter, result, ARGUMENTS);
    abi::load_at_offset(emitter, result, INPUT);
    abi::emit_call_label(emitter, "__rt_incref");
    for offset in [INPUT, OTHER] {
        abi::load_at_offset(emitter, arg1, offset);
        abi::load_at_offset(emitter, arg0, ARGUMENTS);
        if offset == OTHER { clear_slot(emitter, OTHER); }
        abi::emit_call_label(emitter, "__rt_array_push_int");
        abi::store_at_offset(emitter, result, ARGUMENTS);
    }
    value_boxing::emit_box_current_owned_value_as_mixed(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::store_at_offset(emitter, result, ARGUMENTS);
}

/// Clears owner slots before release and resumes cleanup if a callback or destructor throws.
fn emit_cleanup(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.label("__rt_array_udiff_uintersect_cleanup");
    for (offset, helper) in [
        (INPUT, "__rt_decref_mixed"), (OTHER, "__rt_decref_mixed"), (KEY, "__rt_decref_mixed"),
        (ARGUMENTS, "__rt_decref_any"), (NEXT, "__rt_decref_mixed"),
    ] { release_slot(emitter, offset, helper); }
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_udiff_uintersect_release_sources");
    release_slot(emitter, ANSWER, "__rt_decref_mixed");
    emitter.label("__rt_array_udiff_uintersect_release_sources");
    release_slot(emitter, FIRST, "__rt_decref_mixed");
    release_slot(emitter, SECOND, "__rt_decref_mixed");
    release_slot(emitter, CALLBACK, "__rt_callable_descriptor_release");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_udiff_uintersect_return");
    abi::load_at_offset(emitter, value_low_reg(emitter), PREVIOUS);
    clear_slot(emitter, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label("__rt_array_udiff_uintersect_return");
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::load_at_offset(emitter, result, ANSWER);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    emitter.label("__rt_array_udiff_uintersect_caught");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, value_low_reg(emitter), PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_jump(emitter, "__rt_array_udiff_uintersect_cleanup");
}

/// Adapts the iterator's unboxed high word to the retaining Mixed constructor's x86_64 ABI.
fn box_unboxed_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rsi, rdx");                                    // preserve paired payloads before constructing an owned cell
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Registers a native handler before acquiring snapshots or constructing the result.
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
    emitter.bl_c("setjmp");                                                    // preserve every comparator owner across callback and cleanup throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_udiff_uintersect_caught");
}

/// Restores the enclosing handler and warning-suppression depth after all temporary owners retire.
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

/// Clears a frame slot without clobbering the current owner or any of the first four C arguments.
fn clear_slot(emitter: &mut Emitter, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset);
}

/// Retires a previously published owner exactly once, including resumable exceptional cleanup.
fn release_slot(emitter: &mut Emitter, offset: usize, helper: &str) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::emit_call_label(emitter, helper);
}

/// Returns the unboxed low-word and older-exception argument register.
fn value_low_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" }
}

/// Returns the high-word register expected by the retaining Mixed constructor.
fn value_high_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }
}

/// Emits equivalent storage-neutral comparator control flow for each native architecture.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // keep comparator decisions equivalent across targets
}

#[cfg(test)]
mod tests {
    //! Verifies comparator snapshots, descriptor invocation and cleanup on every target.

    use super::*;
    use crate::codegen_support::platform::Target;

    /// The native boundary covers both source snapshots and every callback-owned cell.
    #[test]
    fn boxed_set_comparator_boundary_covers_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_udiff_uintersect(&mut emitter);
            let asm = emitter.output();
            let boundary = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let snapshot = asm.find("__rt_mixed_from_value").unwrap();
            let iterator = asm.find("__rt_array_iter_next").unwrap();
            let callback = asm.find("__rt_callable_invoke_owned_args").unwrap();
            assert!(boundary < snapshot && snapshot < iterator && iterator < callback, "{name}");
            assert!(asm.contains("__rt_mixed_cast_int"), "{name}");
            assert!(asm.contains("__rt_mixed_array_set"), "{name}");
            assert!(asm.contains("__rt_array_udiff_uintersect_caught:"), "{name}");
            assert!(asm.contains("__rt_callable_descriptor_release"), "{name}");
            assert!(asm.contains("__rt_exception_chain"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
            assert!(!asm.contains("__rt_mixed_clone"), "{name}");
        }
    }
}
