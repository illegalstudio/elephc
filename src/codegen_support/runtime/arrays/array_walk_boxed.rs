//! Purpose:
//! Emits storage-neutral `array_walk` and `array_walk_recursive` support for boxed PHP arrays.
//!
//! Called from:
//! - Boxed walk lowering through `__rt_array_walk_boxed`.
//!
//! Key details:
//! - The helper consumes one callback descriptor and roots the caller's separated source cell.
//! - Every visited entry is detached before exposing its storage through an invoker ref marker.
//! - Recursive mode descends through actual runtime array tags and preserves each logical key.
//! - Partially built callback arguments remain owned by the outer exception boundary.
//! - Active entry borrows are registered across visible native callbacks and restored on unwind.
//! - Opaque descriptor kinds fail closed because their reference escapes cannot be audited.

use crate::codegen_support::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen_support::{
    abi, arrays, callable_descriptor, emit::Emitter, platform::Arch, value_boxing,
};
use crate::types::PhpType;

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 96;
const HANDLER: usize = FRAME - 16;
const CALLBACK: usize = 8;
const BORROWED_SOURCE: usize = 16;
const MODE: usize = 24;
const SOURCE: usize = 32;
const PREVIOUS: usize = 40;
const PENDING: usize = 48;
const ARGUMENTS: usize = 56;
const CALLBACK_RESULT: usize = 64;
const BORROW_HEAD: usize = 72;
const ARGUMENT_OWNER_OFFSET: usize = CALLBACK_RESULT - ARGUMENTS;

const VISIT_FRAME: usize = 128;
const VISIT_CALLBACK: usize = 8;
const VISIT_CELL: usize = 16;
const VISIT_MODE: usize = 24;
const VISIT_HASH: usize = 32;
const VISIT_CURSOR: usize = 40;
const VISIT_ENTRY: usize = 48;
const VISIT_KEY_LO: usize = 56;
const VISIT_KEY_HI: usize = 64;
const VISIT_OLD_CELL: usize = 72;
const VISIT_CELL_VALUE: usize = 80;
const VISIT_ARGUMENT_OWNER: usize = 88;
const VISIT_BORROW_CELL: usize = 96;
const VISIT_BORROW_PREVIOUS: usize = 104;

/// Emits the boxed walk owner boundary and the recursive storage-neutral visitor.
///
/// Inputs are an owned callable descriptor, a borrowed separated Mixed array cell, and a
/// recursive-mode flag. The descriptor owner is consumed and the source remains caller-owned.
pub fn emit_array_walk_boxed(emitter: &mut Emitter) {
    emit_entry(emitter);
    emit_visitor(emitter);
}

/// Emits the exception-safe owner boundary around the walk visitor.
fn emit_entry(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.comment("--- runtime: boxed array walk ---");
    emitter.label_global("__rt_array_walk_boxed");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CALLBACK, BORROWED_SOURCE, MODE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [SOURCE, PENDING, ARGUMENTS, CALLBACK_RESULT, BORROW_HEAD] {
        clear_slot(emitter, offset);
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_rt_unmanaged_ref_borrow_top", 0);
    abi::store_at_offset(emitter, result, BORROW_HEAD);
    install_boundary(emitter);
    reject_opaque_callback(emitter);

    abi::load_at_offset(emitter, result, BORROWED_SOURCE);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, SOURCE);
    for (index, offset) in [CALLBACK, SOURCE, MODE].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_frame_slot_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 3),
        CALLBACK_RESULT,
    );
    abi::emit_call_label(emitter, "__rt_array_walk_boxed_visit");
    abi::emit_jump(emitter, "__rt_array_walk_boxed_cleanup");

    emitter.label("__rt_array_walk_boxed_caught");
    restore_entry_borrow_head(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, value_low_reg(emitter), PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");

    emitter.label("__rt_array_walk_boxed_cleanup");
    restore_entry_borrow_head(emitter);
    release_slot(emitter, ARGUMENTS, "__rt_decref_any");
    release_slot(emitter, CALLBACK_RESULT, "__rt_decref_mixed");
    release_slot(emitter, SOURCE, "__rt_decref_mixed");
    release_slot(emitter, CALLBACK, "__rt_callable_descriptor_release");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_walk_boxed_return");
    abi::load_at_offset(emitter, value_low_reg(emitter), PREVIOUS);
    clear_slot(emitter, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label("__rt_array_walk_boxed_return");
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Emits the recursive visitor over a hash kept stable by the outer rooted source graph.
fn emit_visitor(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_walk_boxed_visit");
    abi::emit_frame_prologue(emitter, VISIT_FRAME);
    for (index, offset) in [
        VISIT_CALLBACK,
        VISIT_CELL,
        VISIT_MODE,
        VISIT_ARGUMENT_OWNER,
    ]
        .into_iter()
        .enumerate()
    {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    clear_slot(emitter, VISIT_CURSOR);

    // The top-level Mixed owner keeps the complete graph alive. Each child cell is owned by an
    // ancestor hash until its recursive call returns, while callback writes through another alias
    // split at the rooted outer cell or this promoted child hash before relocating its storage.
    abi::load_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        VISIT_CELL,
    );
    abi::emit_call_label(emitter, "__rt_mixed_cell_promote_to_hash");
    abi::store_at_offset(emitter, result, VISIT_HASH);

    emitter.label("__rt_array_walk_boxed_visit_loop");
    abi::load_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        VISIT_HASH,
    );
    abi::load_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        VISIT_CURSOR,
    );
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    ins(
        emitter,
        "b.eq __rt_array_walk_boxed_visit_return",
        "je __rt_array_walk_boxed_visit_return",
    );
    abi::store_at_offset(emitter, result, VISIT_CURSOR);
    let (key_lo, key_hi, entry) = if emitter.target.arch == Arch::AArch64 {
        ("x1", "x2", "x6")
    } else {
        ("rdi", "rdx", "r10")
    };
    abi::store_at_offset(emitter, key_lo, VISIT_KEY_LO);
    abi::store_at_offset(emitter, key_hi, VISIT_KEY_HI);
    abi::store_at_offset(emitter, entry, VISIT_ENTRY);

    let scratch = scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, VISIT_ENTRY);
    load_indirect(emitter, result, scratch);
    abi::store_at_offset(emitter, result, VISIT_OLD_CELL);
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    abi::store_at_offset(emitter, result, VISIT_CELL_VALUE);
    abi::load_at_offset(emitter, scratch, VISIT_ENTRY);
    store_indirect(emitter, scratch, result);
    abi::load_at_offset(emitter, result, VISIT_OLD_CELL);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    abi::load_at_offset(emitter, result, VISIT_MODE);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_walk_boxed_visit_invoke");
    abi::load_at_offset(emitter, result, VISIT_CELL_VALUE);
    load_indirect(emitter, scratch, result);
    ins(emitter, "sub x10, x10, #4", "sub r10, 4");
    ins(emitter, "cmp x10, #1", "cmp r10, 1");
    ins(
        emitter,
        "b.hi __rt_array_walk_boxed_visit_invoke",
        "ja __rt_array_walk_boxed_visit_invoke",
    );
    for (index, offset) in [
        VISIT_CALLBACK,
        VISIT_CELL_VALUE,
        VISIT_MODE,
        VISIT_ARGUMENT_OWNER,
    ]
        .into_iter()
        .enumerate()
    {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, "__rt_array_walk_boxed_visit");
    abi::emit_jump(emitter, "__rt_array_walk_boxed_visit_loop");

    emitter.label("__rt_array_walk_boxed_visit_invoke");
    prepare_arguments(emitter);
    abi::load_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        VISIT_CALLBACK,
    );
    abi::load_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        VISIT_ARGUMENT_OWNER,
    );
    load_indirect_at(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        abi::int_arg_reg_name(emitter.target, 1),
        ARGUMENT_OWNER_OFFSET,
    );
    abi::load_at_offset(emitter, scratch_reg(emitter), VISIT_ARGUMENT_OWNER);
    store_zero_indirect_at(emitter, scratch_reg(emitter), ARGUMENT_OWNER_OFFSET);
    install_visit_borrow(emitter);
    abi::emit_call_label(emitter, "__rt_callable_invoke_owned_args");
    restore_visit_borrow(emitter);
    store_callback_result(emitter);
    release_callback_result(emitter);
    abi::emit_jump(emitter, "__rt_array_walk_boxed_visit_loop");

    emitter.label("__rt_array_walk_boxed_visit_return");
    abi::emit_frame_restore(emitter, VISIT_FRAME);
    abi::emit_return(emitter);
}

/// Rejects descriptor kinds whose callback body is not visible to native escape guards.
fn reject_opaque_callback(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let kind = scratch_reg(emitter);
    abi::load_at_offset(emitter, result, CALLBACK);
    load_indirect(emitter, kind, result);
    for accepted in [
        callable_descriptor::CALLABLE_DESC_KIND_CLOSURE,
        callable_descriptor::CALLABLE_DESC_KIND_OBJECT_INVOKE,
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        callable_descriptor::CALLABLE_DESC_KIND_INSTANCE_METHOD,
        callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
    ] {
        ins(
            emitter,
            &format!("cmp {kind}, #{accepted}"),
            &format!("cmp {kind}, {accepted}"),
        );
        ins(
            emitter,
            "b.eq __rt_array_walk_boxed_callback_visible",
            "je __rt_array_walk_boxed_callback_visible",
        );
    }
    abi::emit_call_label(emitter, "__rt_unmanaged_reference_escape_error");
    emitter.label("__rt_array_walk_boxed_callback_visible");
}

/// Publishes this visitor's exact borrowed hash-entry slot for nested escape checks.
fn install_visit_borrow(emitter: &mut Emitter) {
    let scratch = scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_rt_unmanaged_ref_borrow_top", 0);
    abi::store_at_offset(emitter, scratch, VISIT_BORROW_PREVIOUS);
    abi::load_at_offset(emitter, scratch, VISIT_ENTRY);
    abi::store_at_offset(emitter, scratch, VISIT_BORROW_CELL);
    abi::emit_frame_slot_address(emitter, scratch, VISIT_BORROW_PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_rt_unmanaged_ref_borrow_top", 0);
}

/// Pops the current visitor node after a callback has returned normally.
fn restore_visit_borrow(emitter: &mut Emitter) {
    let scratch = scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, VISIT_BORROW_PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_rt_unmanaged_ref_borrow_top", 0);
}

/// Restores the chain saved before this walk, including after a callback longjmp.
fn restore_entry_borrow_head(emitter: &mut Emitter) {
    let scratch = scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, BORROW_HEAD);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_rt_unmanaged_ref_borrow_top", 0);
}

/// Builds owned `(value-ref, key)` callback arguments under the outer unwind owner slot.
fn prepare_arguments(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, arg0, 2);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    arrays::emit_array_value_type_stamp(emitter, result, &PhpType::Mixed);
    store_argument_owner(emitter, result);

    abi::emit_load_int_immediate(emitter, result, INVOKER_ARG_REF_CELL_TAG);
    abi::load_at_offset(emitter, value_low_reg(emitter), VISIT_ENTRY);
    abi::emit_load_int_immediate(emitter, value_high_reg(emitter), 7);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    store_argument_element(emitter, result, 0, 1);

    abi::load_at_offset(emitter, value_low_reg(emitter), VISIT_KEY_LO);
    abi::load_at_offset(emitter, value_high_reg(emitter), VISIT_KEY_HI);
    ins(emitter, "cmp x2, #0", "cmp rsi, 0");
    ins(emitter, "cset x0, ge", "setge al");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("movzx eax, al");                                   // integer keys use tag zero and string keys use tag one
    }
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_walk_boxed_string_key");
    abi::emit_load_int_immediate(emitter, value_high_reg(emitter), 0);
    emitter.label("__rt_array_walk_boxed_string_key");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    store_argument_element(emitter, result, 1, 2);
    load_argument_owner(emitter, result);
    value_boxing::emit_box_current_owned_value_as_mixed(
        emitter,
        &PhpType::Array(Box::new(PhpType::Mixed)),
    );
    store_argument_owner(emitter, result);
}

/// Stores the current raw array or boxed Mixed argument owner in the outer boundary slot.
fn store_argument_owner(emitter: &mut Emitter, owner: &str) {
    let slot = scratch_reg(emitter);
    abi::load_at_offset(emitter, slot, VISIT_ARGUMENT_OWNER);
    store_indirect_at(emitter, slot, ARGUMENT_OWNER_OFFSET, owner);
}

/// Loads the current callback argument owner through the outer boundary slot.
fn load_argument_owner(emitter: &mut Emitter, destination: &str) {
    let slot = scratch_reg(emitter);
    abi::load_at_offset(emitter, slot, VISIT_ARGUMENT_OWNER);
    load_indirect_at(emitter, destination, slot, ARGUMENT_OWNER_OFFSET);
}

/// Transfers one fresh Mixed cell into the fixed-capacity callback argument array.
fn store_argument_element(emitter: &mut Emitter, cell: &str, index: usize, length: usize) {
    let array = scratch_reg(emitter);
    let count = abi::tertiary_scratch_reg(emitter);
    load_argument_owner(emitter, array);
    store_indirect_at(emitter, array, 24 + index * 8, cell);
    abi::emit_load_int_immediate(emitter, count, length as i64);
    store_indirect_at(emitter, array, 0, count);
}

/// Roots the callback result in the outer boundary before running its destructor-capable release.
fn store_callback_result(emitter: &mut Emitter) {
    let owners = scratch_reg(emitter);
    abi::load_at_offset(emitter, owners, VISIT_ARGUMENT_OWNER);
    store_indirect(emitter, owners, abi::int_result_reg(emitter));
}

/// Clears and consumes the rooted callback result so a destructor throw can resume cleanup.
fn release_callback_result(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let owners = scratch_reg(emitter);
    abi::load_at_offset(emitter, owners, VISIT_ARGUMENT_OWNER);
    load_indirect(emitter, result, owners);
    let zero = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, zero, 0);
    store_indirect(emitter, owners, zero);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
}

/// Installs a native handler before acquiring the source root or invoking callbacks.
fn install_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_value", PREVIOUS),
        ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        (
            "_rt_diag_suppression",
            HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET,
        ),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        HANDLER - TRY_HANDLER_JMP_BUF_OFFSET,
    );
    emitter.bl_c("setjmp");                                                     // keep descriptor and source owners reachable across callback and cleanup throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_walk_boxed_caught");
}

/// Restores the caller's handler and diagnostic depth after ownership cleanup.
fn restore_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        (
            "_rt_diag_suppression",
            HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET,
        ),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
}

/// Clears an owner slot without clobbering the result register.
fn clear_slot(emitter: &mut Emitter, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset);
}

/// Clears and releases one runtime owner so cleanup can safely resume after a destructor throw.
fn release_slot(emitter: &mut Emitter, offset: usize, helper: &str) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::emit_call_label(emitter, helper);
}

/// Returns the low payload register used by the retaining Mixed constructor.
fn value_low_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 {
        "x1"
    } else {
        "rdi"
    }
}

/// Returns the high payload register used by the retaining Mixed constructor.
fn value_high_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 {
        "x2"
    } else {
        "rsi"
    }
}

/// Returns a scratch register suitable for one-word indirect loads and stores.
fn scratch_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 {
        "x9"
    } else {
        "r10"
    }
}

/// Loads one pointer-sized word through an address register.
fn load_indirect(emitter: &mut Emitter, destination: &str, address: &str) {
    load_indirect_at(emitter, destination, address, 0);
}

/// Loads one pointer-sized word through an address register plus a byte offset.
fn load_indirect_at(emitter: &mut Emitter, destination: &str, address: &str, offset: usize) {
    let instruction = if emitter.target.arch == Arch::AArch64 {
        format!("ldr {destination}, [{address}, #{offset}]")
    } else {
        format!("mov {destination}, QWORD PTR [{address} + {offset}]")
    };
    emitter.instruction(&instruction);                                          // load the current boxed cell or its runtime tag
}

/// Stores one pointer-sized word through an address register.
fn store_indirect(emitter: &mut Emitter, address: &str, source: &str) {
    store_indirect_at(emitter, address, 0, source);
}

/// Stores one pointer-sized word through an address register plus a byte offset.
fn store_indirect_at(emitter: &mut Emitter, address: &str, offset: usize, source: &str) {
    let instruction = if emitter.target.arch == Arch::AArch64 {
        format!("str {source}, [{address}, #{offset}]")
    } else {
        format!("mov QWORD PTR [{address} + {offset}], {source}")
    };
    emitter.instruction(&instruction);                                          // publish a transferred owner before another operation can throw
}

/// Clears one pointer-sized owner slot reached through an address plus a byte offset.
fn store_zero_indirect_at(emitter: &mut Emitter, address: &str, offset: usize) {
    let zero = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, zero, 0);
    store_indirect_at(emitter, address, offset, zero);
}

/// Emits equivalent visitor control flow for either supported architecture.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    let instruction = if emitter.target.arch == Arch::AArch64 {
        arm
    } else {
        x86
    };
    emitter.instruction(instruction);                                           // preserve boxed walk decisions across every supported target
}

#[cfg(test)]
mod tests {
    //! Verifies boxed walk mutation, descriptor, and exception wiring for all supported targets.

    use super::*;
    use crate::codegen_support::platform::Target;

    /// Pins COW promotion, stable ref markers, direct ownership transfer, and unwind cleanup.
    #[test]
    fn boxed_walk_runtime_is_complete_for_every_target() {
        assert_eq!(FRAME % 16, 0);
        assert_eq!(VISIT_FRAME % 16, 0);
        assert!(HANDLER - TRY_HANDLER_SLOT_SIZE >= BORROW_HEAD);
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_walk_boxed(&mut emitter);
            let asm = emitter.output();
            let handler = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let promote = asm.find("__rt_mixed_cell_promote_to_hash").unwrap();
            let detach = asm.find("__rt_mixed_clone").unwrap();
            let invoke = asm.find("__rt_callable_invoke_owned_args").unwrap();
            assert!(handler < promote && promote < detach && detach < invoke, "{name}");
            assert_eq!(asm.matches("__rt_mixed_cell_promote_to_hash").count(), 1, "{name}");
            assert!(asm.contains("__rt_hash_iter_next"), "{name}");
            let marker = if target.arch == Arch::X86_64 {
                format!("mov rax, {INVOKER_ARG_REF_CELL_TAG}")
            } else {
                format!("mov x0, #{INVOKER_ARG_REF_CELL_TAG}")
            };
            assert!(asm.contains(&marker), "{name}");
            assert!(asm.contains("__rt_callable_descriptor_release"), "{name}");
            assert!(asm.contains("__rt_decref_any"), "{name}");
            assert!(asm.contains("__rt_exception_chain"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
            assert!(asm.contains("__rt_array_walk_boxed_visit"), "{name}");
            assert!(asm.contains("__rt_unmanaged_reference_escape_error"), "{name}");
            assert!(asm.matches("_rt_unmanaged_ref_borrow_top").count() >= 5, "{name}");
            let callback_gate = asm
                .split_once("__rt_array_walk_boxed:")
                .unwrap()
                .1
                .split_once("__rt_array_walk_boxed_callback_visible:")
                .unwrap()
                .0;
            for accepted in [
                callable_descriptor::CALLABLE_DESC_KIND_CLOSURE,
                callable_descriptor::CALLABLE_DESC_KIND_OBJECT_INVOKE,
                callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
                callable_descriptor::CALLABLE_DESC_KIND_INSTANCE_METHOD,
                callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            ] {
                let compare = if target.arch == Arch::X86_64 {
                    format!("cmp r10, {accepted}")
                } else {
                    format!("cmp x9, #{accepted}")
                };
                assert!(callback_gate.contains(&compare), "{name}: {asm}");
            }
            assert!(!asm.contains("__rt_array_push_int"), "{name}: fixed callback arguments transfer directly");
            let transferred = if target.arch == Arch::X86_64 {
                "mov QWORD PTR [r10 + 24], rax"
            } else {
                "str x0, [x9, #24]"
            };
            assert!(asm.contains(transferred), "{name}: {asm}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("sub x10, x10, #4"), "{name}: {asm}");
                assert!(asm.contains("cmp x10, #1"), "{name}: {asm}");
            }
            let owner_pair = if target.arch == Arch::X86_64 {
                "lea rcx, [rbp - 64]"
            } else {
                "sub x3, x29, #64"
            };
            assert!(asm.contains(owner_pair), "{name}: {asm}");
            let argument_load = if target.arch == Arch::X86_64 {
                "mov rsi, QWORD PTR [rsi + 8]"
            } else {
                "ldr x1, [x1, #8]"
            };
            let invoke_path = asm.split_once("__rt_array_walk_boxed_visit_invoke:").unwrap().1;
            let loaded = invoke_path.find(argument_load).expect("load the argument owner, not the result slot");
            let invoked = invoke_path.find("__rt_callable_invoke_owned_args").unwrap();
            assert!(loaded < invoked, "{name}: {asm}");
            let registered = invoke_path[..invoked].rfind("_rt_unmanaged_ref_borrow_top").unwrap();
            let restored = invoke_path[invoked..].find("_rt_unmanaged_ref_borrow_top").unwrap();
            assert!(registered < invoked && restored > 0, "{name}: {asm}");
        }
    }
}
