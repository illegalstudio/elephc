use super::*;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform, Target};
use crate::types::PhpType;

fn test_emitter() -> Emitter {
    Emitter::new(Target::new(Platform::MacOS, Arch::AArch64))
}

fn test_emitter_x86() -> Emitter {
    Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
}

#[test]
fn test_emit_frame_helpers_small_frame() {
    let mut emitter = test_emitter();
    emit_frame_prologue(&mut emitter, 64);
    emit_frame_restore(&mut emitter, 64);
    emit_return(&mut emitter);

    assert_eq!(
        emitter.output(),
        concat!(
            "    ; prologue\n",
            "    sub sp, sp, #64\n",
            "    stp x29, x30, [sp, #48]\n",
            "    add x29, sp, #48\n",
            "    ldp x29, x30, [sp, #48]\n",
            "    add sp, sp, #64\n",
            "    ret\n",
        )
    );
}

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
fn test_emit_preserve_and_restore_return_value_for_strings() {
    let mut emitter = test_emitter();
    emit_preserve_return_value(&mut emitter, &PhpType::Str, 32);
    emit_restore_return_value(&mut emitter, &PhpType::Str, 32);

    assert_eq!(
        emitter.output(),
        concat!(
            "    stur x1, [x29, #-32]\n",
            "    stur x2, [x29, #-24]\n",
            "    ldur x1, [x29, #-32]\n",
            "    ldur x2, [x29, #-24]\n",
        )
    );
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
    assert_eq!(assignments[8].start_reg, super::registers::STACK_ARG_SENTINEL);
    assert!(assignments[9].is_float);
    assert_eq!(assignments[16].start_reg, 7);
    assert_eq!(assignments[17].start_reg, super::registers::STACK_ARG_SENTINEL);
}

#[test]
fn test_build_outgoing_arg_assignments_for_linux_x86_64_respects_sysv_limits() {
    let assignments = build_outgoing_arg_assignments_for_target(
        Target::new(Platform::Linux, Arch::X86_64),
        &[
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Int,
            PhpType::Str,
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
    assert_eq!(assignments[5].start_reg, 5);
    assert_eq!(assignments[6].start_reg, super::registers::STACK_ARG_SENTINEL);
    assert_eq!(assignments[7].start_reg, super::registers::STACK_ARG_SENTINEL);
    assert!(assignments[8].is_float);
    assert_eq!(assignments[15].start_reg, 7);
    assert_eq!(assignments[16].start_reg, super::registers::STACK_ARG_SENTINEL);
}

#[test]
fn test_incoming_arg_cursor_for_linux_x86_64_uses_sysv_defaults() {
    let cursor = IncomingArgCursor::for_target(Target::new(Platform::Linux, Arch::X86_64), 0);
    assert_eq!(cursor.caller_stack_offset, 16);
    assert!(!cursor.int_stack_only);

    let stack_only_cursor =
        IncomingArgCursor::for_target(Target::new(Platform::Linux, Arch::X86_64), 6);
    assert!(stack_only_cursor.int_stack_only);
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
fn test_materialize_outgoing_args_for_linux_x86_64_uses_sysv_registers() {
    let mut emitter = test_emitter_x86();
    let assignments = build_outgoing_arg_assignments_for_target(
        Target::new(Platform::Linux, Arch::X86_64),
        &[
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
    assert!(out.contains("    sub rsp, 16\n"));
    assert!(out.contains("    mov rdi, QWORD PTR [rsp + 112]\n"));
    assert!(out.contains("    mov r9, QWORD PTR [rsp + 32]\n"));
    assert!(out.contains("    mov r10, QWORD PTR [rsp + 16]\n"));
    assert!(out.contains("    mov QWORD PTR [rsp + 112], r10\n"));
    assert!(out.contains("    add rsp, 112\n"));
}

#[test]
fn test_emit_symbol_address_uses_platform_relocations() {
    let mut emitter = test_emitter();
    emit_symbol_address(&mut emitter, "x9", "_demo_symbol");

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_symbol@PAGE\n",
            "    add x9, x9, _demo_symbol@PAGEOFF\n",
        )
    );
}

#[test]
fn test_emit_store_and_load_result_to_symbol_for_string() {
    let mut emitter = test_emitter();
    emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Str, false);
    emit_load_symbol_to_result(&mut emitter, "_demo_symbol", &PhpType::Str);
    let out = emitter.output();

    assert!(out.contains("    str x1, [x9]\n"));
    assert!(out.contains("    str x2, [x9, #8]\n"));
    assert!(out.contains("    ldr x1, [x9]\n"));
    assert!(out.contains("    ldr x2, [x9, #8]\n"));
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

#[test]
fn test_emit_frame_helpers_linux_x86_64() {
    let mut emitter = test_emitter_x86();
    emit_frame_prologue(&mut emitter, 48);
    emit_frame_restore(&mut emitter, 48);
    emit_return(&mut emitter);

    assert_eq!(
        emitter.output(),
        concat!(
            "    # prologue\n",
            "    push rbp\n",
            "    mov rbp, rsp\n",
            "    sub rsp, 32\n",
            "    add rsp, 32\n",
            "    pop rbp\n",
            "    ret\n",
        )
    );
}

#[test]
fn test_emit_frame_slot_address_linux_x86_64() {
    let mut emitter = test_emitter_x86();
    emit_frame_slot_address(&mut emitter, "r10", 40);

    assert_eq!(emitter.output(), "    lea r10, [rbp - 40]\n");
}

#[test]
fn test_emit_symbol_address_uses_rip_relative_on_linux_x86_64() {
    let mut emitter = test_emitter_x86();
    emit_symbol_address(&mut emitter, "r11", "_demo_symbol");

    assert_eq!(emitter.output(), "    lea r11, [rip + _demo_symbol]\n");
}

#[test]
fn test_emit_store_and_load_result_to_symbol_for_string_linux_x86_64() {
    let mut emitter = test_emitter_x86();
    emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Str, false);
    emit_load_symbol_to_result(&mut emitter, "_demo_symbol", &PhpType::Str);
    let out = emitter.output();

    assert!(out.contains("    mov QWORD PTR [rip + _demo_symbol], rax\n"));
    assert!(out.contains("    mov QWORD PTR [r11 + 8], rdx\n"));
    assert!(out.contains("    mov rax, QWORD PTR [rip + _demo_symbol]\n"));
    assert!(out.contains("    mov rdx, QWORD PTR [r11 + 8]\n"));
}

#[test]
fn test_process_entry_helpers_linux_x86_64() {
    let mut emitter = test_emitter_x86();

    emit_store_process_args_to_globals(&mut emitter);
    emit_enable_heap_debug_flag(&mut emitter);
    emit_copy_frame_pointer(&mut emitter, "r10");
    emit_exit(&mut emitter, 7);

    let out = emitter.output();

    assert!(out.contains("    mov QWORD PTR [rip + _global_argc], rdi\n"));
    assert!(out.contains("    mov QWORD PTR [rip + _global_argv], rsi\n"));
    assert!(out.contains("    mov r10, 1\n"));
    assert!(out.contains("    mov QWORD PTR [rip + _heap_debug_enabled], r10\n"));
    assert!(out.contains("    mov r10, rbp\n"));
    assert!(out.contains("    mov edi, 7\n"));
    assert!(out.contains("    mov eax, 60\n"));
    assert!(out.contains("    syscall\n"));
}

#[test]
fn test_emit_store_incoming_param_linux_x86_64_uses_sysv_registers_and_stack() {
    let mut emitter = test_emitter_x86();
    let mut cursor =
        IncomingArgCursor::for_target(Target::new(Platform::Linux, Arch::X86_64), 0);

    emit_store_incoming_param(&mut emitter, "a", &PhpType::Int, 8, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "b", &PhpType::Float, 16, false, &mut cursor);
    emit_store_incoming_param(&mut emitter, "c", &PhpType::Str, 32, false, &mut cursor);

    let mut stack_cursor =
        IncomingArgCursor::for_target(Target::new(Platform::Linux, Arch::X86_64), 6);
    emit_store_incoming_param(&mut emitter, "d", &PhpType::Int, 40, false, &mut stack_cursor);

    let out = emitter.output();

    assert!(out.contains("    # param $a from rdi\n"));
    assert!(out.contains("    mov QWORD PTR [rbp - 8], rdi\n"));
    assert!(out.contains("    # param $b from xmm0\n"));
    assert!(out.contains("    movsd QWORD PTR [rbp - 16], xmm0\n"));
    assert!(out.contains("    # param $c from rsi,rdx\n"));
    assert!(out.contains("    mov QWORD PTR [rbp - 32], rsi\n"));
    assert!(out.contains("    mov QWORD PTR [rbp - 24], rdx\n"));
    assert!(out.contains("    # param $d from caller stack +16\n"));
    assert!(out.contains("    mov r10, QWORD PTR [rbp + 16]\n"));
    assert!(out.contains("    mov QWORD PTR [rbp - 40], r10\n"));
}

#[test]
fn test_emit_call_and_temporary_stack_helpers_linux_x86_64() {
    let mut emitter = test_emitter_x86();

    emit_push_reg(&mut emitter, "r12");
    super::calls::emit_pop_reg(&mut emitter, "r12");
    super::calls::emit_push_float_reg(&mut emitter, "xmm3");
    super::calls::emit_push_reg_pair(&mut emitter, "rax", "rdx");
    emit_call_label(&mut emitter, "_fn_demo");
    emit_call_reg(&mut emitter, "r12");
    emit_release_temporary_stack(&mut emitter, 32);
    emit_store_zero_to_local_slot(&mut emitter, 24);

    assert_eq!(
        emitter.output(),
        concat!(
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], r12\n",
            "    mov r12, QWORD PTR [rsp]\n",
            "    add rsp, 16\n",
            "    sub rsp, 16\n",
            "    movsd QWORD PTR [rsp], xmm3\n",
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], rax\n",
            "    mov QWORD PTR [rsp + 8], rdx\n",
            "    call _fn_demo\n",
            "    call r12\n",
            "    add rsp, 32\n",
            "    mov QWORD PTR [rbp - 24], 0\n",
        )
    );
}

#[test]
fn test_emit_push_result_value_linux_x86_64_uses_native_result_registers() {
    let mut emitter = test_emitter_x86();

    emit_push_result_value(&mut emitter, &PhpType::Int);
    emit_push_result_value(&mut emitter, &PhpType::Float);
    emit_push_result_value(&mut emitter, &PhpType::Str);

    assert_eq!(
        emitter.output(),
        concat!(
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], rax\n",
            "    sub rsp, 16\n",
            "    movsd QWORD PTR [rsp], xmm0\n",
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], rax\n",
            "    mov QWORD PTR [rsp + 8], rdx\n",
        )
    );
}
