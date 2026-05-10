//! Purpose:
//! Contains ABI regression tests for arguments helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

#[test]
fn test_emit_store_incoming_param_uses_registers_then_caller_stack() {
    let mut emitter = test_emitter();
    let mut cursor = IncomingArgCursor::default();

    emit_store_incoming_param(&mut emitter, "a", &PhpType::Int, 8, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "b", &PhpType::Int, 16, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "c", &PhpType::Int, 24, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "d", &PhpType::Int, 32, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "e", &PhpType::Int, 40, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "f", &PhpType::Int, 48, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "g", &PhpType::Int, 56, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "h", &PhpType::Int, 64, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "i", &PhpType::Int, 72, false, &mut cursor);

    let out = emitter.output();
    assert!(out.contains("    ; param $h from x7\n"));
    assert!(out.contains("    ; param $i from caller stack +32\n"));
    assert!(out.contains("    ldr x10, [x29, #32]\n"));
}

#[test]
fn test_emit_frame_slot_address_large_offset() {
    let mut emitter = test_emitter();
    emit_frame_slot_address(&mut emitter, "x0", 5000);

    assert_eq!(
        emitter.output(),
        concat!(
            "    mov x0, x29\n",
            "    sub x0, x0, #4095\n",
            "    sub x0, x0, #905\n",
        )
    );
}

#[test]
fn test_build_outgoing_arg_assignments_respects_register_limits() {
    let assignments = build_outgoing_arg_assignments_for_target(
        Target::new(Platform::MacOS, Arch::AArch64),
        &[
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
            PhpType::Float,
        ],
        0,
    );

    assert_eq!(assignments[0].start_reg, 0);
    assert_eq!(assignments[7].start_reg, 7);
    assert_eq!(assignments[8].start_reg, crate::codegen::abi::registers::STACK_ARG_SENTINEL);
    assert!(assignments[9].is_float);
    assert_eq!(assignments[16].start_reg, 7);
    assert_eq!(assignments[17].start_reg, crate::codegen::abi::registers::STACK_ARG_SENTINEL);
}

#[test]
fn test_materialize_outgoing_args_keeps_overflow_on_stack() {
    let mut emitter = test_emitter();
    let assignments = build_outgoing_arg_assignments_for_target(
        Target::new(Platform::MacOS, Arch::AArch64),
        &[
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
        ],
        0,
    );

    let overflow_bytes = materialize_outgoing_args(&mut emitter, &assignments);
    let out = emitter.output();

    assert_eq!(overflow_bytes, 16);
    assert!(out.contains("    sub sp, sp, #16\n"));
    assert!(out.contains("    ldr x0, [sp, #144]\n"));
    assert!(out.contains("    ldr x7, [sp, #32]\n"));
    assert!(out.contains("    str x10, [sp, #144]\n"));
    assert!(out.contains("    add sp, sp, #144\n"));
}

#[test]
fn test_emit_store_local_slot_to_symbol_handles_large_string_slot() {
    let mut emitter = test_emitter();
    emit_store_local_slot_to_symbol(&mut emitter, "_static_demo_name", &PhpType::Str, 5000);
    let out = emitter.output();

    assert!(out.contains("    adrp x9, _static_demo_name@PAGE\n"));
    assert!(out.contains("    add x9, x9, _static_demo_name@PAGEOFF\n"));
    assert!(out.contains("    mov x9, x29\n"));
    assert!(out.contains("    sub x9, x9, #4095\n"));
    assert!(out.contains("    ldr x10, [x9]\n"));
    assert!(out.contains("    ldr x11, [x9]\n"));
    assert!(out.contains("    str x10, [x9]\n"));
    assert!(out.contains("    str x11, [x9, #8]\n"));
}

#[test]
fn test_emit_load_symbol_to_local_slot_handles_large_string_slot() {
    let mut emitter = test_emitter();
    emit_load_symbol_to_local_slot(&mut emitter, "_static_demo_name", &PhpType::Str, 5000);
    let out = emitter.output();

    assert!(out.contains("    adrp x9, _static_demo_name@PAGE\n"));
    assert!(out.contains("    add x9, x9, _static_demo_name@PAGEOFF\n"));
    assert!(out.contains("    ldr x1, [x9]\n"));
    assert!(out.contains("    ldr x2, [x9, #8]\n"));
    assert!(out.contains("    mov x10, x29\n"));
    assert!(out.contains("    sub x10, x10, #4095\n"));
    assert!(out.contains("    str x1, [x10]\n"));
    assert!(out.contains("    mov x11, x29\n"));
    assert!(out.contains("    sub x11, x11, #4095\n"));
    assert!(out.contains("    str x2, [x11]\n"));
}
