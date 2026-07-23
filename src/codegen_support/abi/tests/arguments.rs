//! Purpose:
//! Contains ABI regression tests for arguments helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen_support::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

/// Verifies that incoming AArch64 integer parameters use x0-x7, then caller stack slots.
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

/// Verifies that an incoming AArch64 float overflow uses caller-saved d31 rather than d15.
#[test]
fn test_emit_store_incoming_float_overflow_uses_volatile_scratch() {
    let mut emitter = test_emitter();
    let mut cursor = IncomingArgCursor::default();

    for (index, name) in ["a", "b", "c", "d", "e", "f", "g", "h", "i"]
        .iter()
        .enumerate()
    {
        emit_store_incoming_param(
            &mut emitter,
            name,
            &PhpType::Float,
            (index + 1) * 8,
            false,
            &mut cursor,
        );
    }

    let out = emitter.output();
    assert!(out.contains("    ; param $i from caller stack +32\n"));
    assert!(out.contains("    ldr d31, [x29, #32]\n"));
    assert!(!out.contains("d15"));
}

/// Tests that `emit_frame_slot_address` correctly handles offsets larger than
/// a single immediate instruction can encode by emitting multiple sub instructions
/// to reach offsets like 5000 on ARM64.
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

/// Tests that `build_outgoing_arg_assignments_for_target` respects register limits
/// on ARM64, using registers x0-x7 for the first 8 integer args and spilling
/// subsequent args to the stack. Also verifies float args (d0-d7) are tracked
/// with is_float=true and correctly overflow to stack after 8 float registers.
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
    assert_eq!(assignments[8].start_reg, crate::codegen_support::abi::registers::STACK_ARG_SENTINEL);
    assert!(assignments[9].is_float);
    assert_eq!(assignments[16].start_reg, 7);
    assert_eq!(assignments[17].start_reg, crate::codegen_support::abi::registers::STACK_ARG_SENTINEL);
}

/// Verifies that outgoing stack arguments are staged without overlapping temp slots.
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
    assert!(out.contains("    sub sp, sp, #32\n"));
    assert!(out.contains("    ldr x0, [sp, #160]\n"));
    assert!(out.contains("    ldr x7, [sp, #48]\n"));
    assert!(out.contains("    ldr x10, [sp, #32]\n"));
    assert!(out.contains("    str x10, [sp]\n"));
    assert!(out.contains("    ldr x10, [sp]\n"));
    assert!(out.contains("    str x10, [sp, #160]\n"));
    assert!(out.contains("    add sp, sp, #160\n"));
}

/// Verifies that outgoing AArch64 float overflow staging never clobbers callee-saved d15.
#[test]
fn test_materialize_outgoing_float_overflow_uses_volatile_scratch() {
    let mut emitter = test_emitter();
    let arg_types = vec![PhpType::Float; 9];
    let assignments = build_outgoing_arg_assignments_for_target(
        Target::new(Platform::MacOS, Arch::AArch64),
        &arg_types,
        0,
    );

    let overflow_bytes = materialize_outgoing_args(&mut emitter, &assignments);
    let out = emitter.output();

    assert_eq!(overflow_bytes, 16);
    assert!(out.contains("    ldr d31, [sp, #32]\n"));
    assert!(out.contains("    str d31, [sp]\n"));
    assert!(!out.contains("d15"));
}

/// Verifies Windows calls always reserve the mandatory four-slot caller home area.
#[test]
fn test_windows_outgoing_call_stack_pad_reserves_shadow_space_without_overflow() {
    let target = Target::new(Platform::Windows, Arch::X86_64);
    assert_eq!(outgoing_call_stack_pad_bytes(target, 0), 32);
}

/// Verifies Windows overflow arguments remain positioned after the fixed shadow area.
#[test]
fn test_windows_outgoing_call_stack_pad_precedes_overflow_slots() {
    let target = Target::new(Platform::Windows, Arch::X86_64);
    assert_eq!(outgoing_call_stack_pad_bytes(target, 32), 32);
}

/// Verifies that native Windows C arguments share four positional register slots.
#[test]
fn test_windows_c_abi_mixed_arguments_use_positional_register_slots() {
    let assignments = build_c_abi_outgoing_arg_assignments_for_target(
        Target::new(Platform::Windows, Arch::X86_64),
        &[
            PhpType::Int,
            PhpType::Float,
            PhpType::Int,
            PhpType::Float,
            PhpType::Int,
        ],
    );

    assert_eq!(assignments[0].start_reg, 0);
    assert_eq!(assignments[1].start_reg, 1);
    assert_eq!(assignments[2].start_reg, 2);
    assert_eq!(assignments[3].start_reg, 3);
    assert_eq!(assignments[4].start_reg, crate::codegen_support::abi::registers::STACK_ARG_SENTINEL);
    assert!(!assignments[0].is_float);
    assert!(assignments[1].is_float);
}

/// Verifies that non-Windows native C calls retain the regular target ABI plan.
#[test]
fn test_non_windows_c_abi_plan_matches_regular_plan() {
    let target = Target::new(Platform::Linux, Arch::X86_64);
    let types = [PhpType::Int, PhpType::Float, PhpType::Int];

    assert_eq!(
        build_c_abi_outgoing_arg_assignments_for_target(target, &types),
        build_outgoing_arg_assignments_for_target(target, &types, 0)
    );
}

/// Verifies that MSx64 overflow slots are compacted from PHP's 16-byte staging layout.
#[test]
fn test_windows_c_abi_compacts_overflow_to_eight_byte_slots() {
    let target = Target::new(Platform::Windows, Arch::X86_64);
    let mut emitter = Emitter::new(target);
    let assignments = build_c_abi_outgoing_arg_assignments_for_target(
        target,
        &[
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Float,
            PhpType::Int,
        ],
    );

    compact_windows_c_abi_stack_args(&mut emitter, &assignments);
    let out = emitter.output();

    assert!(out.contains("movsd xmm15, QWORD PTR [rsp + 16]"));
    assert!(out.contains("movsd QWORD PTR [rsp + 8], xmm15"));
    assert!(out.contains("mov r10, QWORD PTR [rsp + 32]"));
    assert!(out.contains("mov QWORD PTR [rsp + 16], r10"));
}

/// Verifies elephc's Windows callee reads owned overflow shapes after the full
/// MSx64 shadow/return-address/saved-rbp prefix and advances in 16-byte slots.
#[test]
fn test_windows_incoming_owned_overflow_shapes_use_internal_stack_slots() {
    let target = Target::new(Platform::Windows, Arch::X86_64);
    let mut emitter = Emitter::new(target);
    let mut cursor = IncomingArgCursor::for_target(target, 0);
    for (index, name) in ["a", "b", "c", "d"].iter().enumerate() {
        emit_store_incoming_param(
            &mut emitter,
            name,
            &PhpType::Int,
            (index + 1) * 16,
            false,
            &mut cursor,
        );
    }
    emit_store_incoming_param(
        &mut emitter,
        "label",
        &PhpType::Str,
        80,
        false,
        &mut cursor,
    );
    emit_store_incoming_param(
        &mut emitter,
        "object",
        &PhpType::Object("Payload".to_string()),
        96,
        false,
        &mut cursor,
    );
    emit_store_incoming_param(
        &mut emitter,
        "mixed",
        &PhpType::Mixed,
        112,
        false,
        &mut cursor,
    );

    let out = emitter.output();
    assert!(out.contains("param $label from caller stack +48"));
    assert!(out.contains("mov r10, QWORD PTR [rbp + 48]"), "{out}");
    assert!(out.contains("mov rcx, QWORD PTR [rbp + 56]"), "{out}");
    assert!(out.contains("param $object from caller stack +64"));
    assert!(out.contains("param $mixed from caller stack +80"));
}

/// Tests that `emit_store_local_slot_to_symbol` handles string slots with large
/// offsets (>4095) by emitting the necessary adrp/add page calculations and
/// decomposed sub instructions to reach the slot, then stores both x10 and x11
/// (the string pointer and length halves) at the computed address.
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

/// Tests that `emit_load_symbol_to_local_slot` handles string slots with large
/// offsets (>4095) by emitting the necessary adrp/add page calculations and
/// decomposed sub instructions to reach the slot, then loads x1 and x2 (the
/// string pointer and length halves) from the symbol and stores them at the
/// computed local slot address.
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
