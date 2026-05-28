//! Purpose:
//! Contains ABI regression tests for linux x86 64 helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

#[test]
fn test_build_outgoing_arg_assignments_for_linux_x86_64_respects_sysv_limits() {
    // Verifies that build_outgoing_arg_assignments_for_target applies SysV AMD64 ABI
    // rules: integer arguments use registers rdi, rsi, rdx, rcx, r8, r9 (indices 0-5),
    // floating-point arguments use xmm0-xmm7 (indices 0-7), and arguments beyond those
    // limits are assigned the STACK_ARG_SENTINEL sentinel indicating stack placement.
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
    assert_eq!(assignments[6].start_reg, crate::codegen::abi::registers::STACK_ARG_SENTINEL);
    assert_eq!(assignments[7].start_reg, crate::codegen::abi::registers::STACK_ARG_SENTINEL);
    assert!(assignments[8].is_float);
    assert_eq!(assignments[15].start_reg, 7);
    assert_eq!(assignments[16].start_reg, crate::codegen::abi::registers::STACK_ARG_SENTINEL);
}

#[test]
fn test_incoming_arg_cursor_for_linux_x86_64_uses_sysv_defaults() {
    // Verifies that IncomingArgCursor::for_target initializes with correct SysV AMD64
    // defaults: caller_stack_offset is 16 (space for return address + saved rbp),
    // int_stack_only is false for the first 6 integer register slots, and the cursor
    // transitions to int_stack_only mode when argument index exceeds 5 (the last
    // integer register slot). Caller stack offset is 16.
    let cursor = IncomingArgCursor::for_target(Target::new(Platform::Linux, Arch::X86_64), 0);
    assert_eq!(cursor.caller_stack_offset, 16);
    assert!(!cursor.int_stack_only);

    let stack_only_cursor =
        IncomingArgCursor::for_target(Target::new(Platform::Linux, Arch::X86_64), 6);
    assert!(stack_only_cursor.int_stack_only);
}

/// Verifies that Linux x86_64 outgoing integer args use SysV registers and staged stack overflow.
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
    assert!(out.contains("    sub rsp, 32\n"));
    assert!(out.contains("    mov rdi, QWORD PTR [rsp + 128]\n"));
    assert!(out.contains("    mov r9, QWORD PTR [rsp + 48]\n"));
    assert!(out.contains("    mov r10, QWORD PTR [rsp + 32]\n"));
    assert!(out.contains("    mov QWORD PTR [rsp], r10\n"));
    assert!(out.contains("    mov r10, QWORD PTR [rsp]\n"));
    assert!(out.contains("    mov QWORD PTR [rsp + 128], r10\n"));
    assert!(out.contains("    add rsp, 128\n"));
}

/// Verifies that Linux x86_64 outgoing string args preserve register temps while staging overflow.
#[test]
fn test_materialize_outgoing_string_args_for_linux_x86_64_preserves_live_rcx() {
    let mut emitter = test_emitter_x86();
    let assignments = build_outgoing_arg_assignments_for_target(
        Target::new(Platform::Linux, Arch::X86_64),
        &[PhpType::Str, PhpType::Str, PhpType::Str],
        1,
    );

    let overflow_bytes = materialize_outgoing_args(&mut emitter, &assignments);
    let out = emitter.output();

    assert_eq!(overflow_bytes, 16);
    assert!(out.contains("    mov rsi, QWORD PTR [rsp + 64]\n"));
    assert!(out.contains("    mov rdx, QWORD PTR [rsp + 72]\n"));
    assert!(out.contains("    mov rcx, QWORD PTR [rsp + 48]\n"));
    assert!(out.contains("    mov r8, QWORD PTR [rsp + 56]\n"));
    assert!(out.contains("    mov r10, QWORD PTR [rsp + 32]\n"));
    assert!(out.contains("    mov r11, QWORD PTR [rsp + 40]\n"));
    assert!(out.contains("    mov QWORD PTR [rsp], r10\n"));
    assert!(out.contains("    mov QWORD PTR [rsp + 8], r11\n"));
    assert!(out.contains("    mov QWORD PTR [rsp + 64], r10\n"));
    assert!(out.contains("    mov QWORD PTR [rsp + 72], r11\n"));
    assert!(!out.contains("    mov rcx, QWORD PTR [rsp + 40]\n"));
}

#[test]
fn test_emit_frame_helpers_linux_x86_64() {
    // Verifies that emit_frame_prologue emits push rbp / mov rbp, rsp / sub rsp, N
    // for the x86_64 frame setup; emit_frame_restore emits add rsp, N / pop rbp;
    // and emit_return emits the standard epilogue with ret. The test confirms the
    // 16-byte stack alignment requirement is respected.
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
    // Verifies that emit_frame_slot_address emits "lea reg, [rbp - offset]" to
    // compute the address of a local variable slot relative to the frame pointer.
    // On x86_64, negative offsets from rbp access locals; the lea instruction
    // computes the address without touching memory.
    let mut emitter = test_emitter_x86();
    emit_frame_slot_address(&mut emitter, "r10", 40);

    assert_eq!(emitter.output(), "    lea r10, [rbp - 40]\n");
}

#[test]
fn test_emit_load_and_store_to_address_linux_x86_64() {
    // Verifies that emit_load_from_address emits mov for integer and movsd for
    // floating-point loads from a base+offset address; emit_store_to_address
    // emits the corresponding store instructions; and emit_store_zero_to_address
    // emits a mov with immediate zero. Tests both integer (QWORD PTR) and
    // floating-point (movsd) variants at different offsets.
    let mut emitter = test_emitter_x86();
    emit_load_from_address(&mut emitter, "rax", "r11", 0);
    emit_load_from_address(&mut emitter, "xmm0", "r11", 8);
    emit_store_to_address(&mut emitter, "r10", "r11", 0);
    emit_store_to_address(&mut emitter, "xmm1", "r11", 8);
    emit_store_zero_to_address(&mut emitter, "r11", 16);

    assert_eq!(
        emitter.output(),
        concat!(
            "    mov rax, QWORD PTR [r11]\n",
            "    movsd xmm0, QWORD PTR [r11 + 8]\n",
            "    mov QWORD PTR [r11], r10\n",
            "    movsd QWORD PTR [r11 + 8], xmm1\n",
            "    mov QWORD PTR [r11 + 16], 0\n",
        )
    );
}

#[test]
fn test_emit_symbol_address_uses_rip_relative_on_linux_x86_64() {
    // Verifies that emit_symbol_address uses RIP-relative LEA on x86_64 (the
    // standard position-independent code pattern: lea reg, [rip + symbol]).
    // RIP-relative addressing is the standard ABI way to access global symbols
    // in position-independent code without needing a GOT entry.
    let mut emitter = test_emitter_x86();
    emit_symbol_address(&mut emitter, "r11", "_demo_symbol");

    assert_eq!(emitter.output(), "    lea r11, [rip + _demo_symbol]\n");
}

#[test]
fn test_emit_extern_symbol_address_uses_gotpcrel_on_linux_x86_64() {
    // Verifies that extern symbol addresses are accessed via GOTPCREL on x86_64.
    // The emitted instruction is "mov reg, QWORD PTR symbol@GOTPCREL[rip]" which
    // loads the symbol's address from the Global Offset Table using a PC-relative
    // relocation. This is the standard PIC mechanism for accessing symbols
    // imported from shared libraries.
    let mut emitter = test_emitter_x86();
    crate::codegen::abi::symbols::emit_extern_symbol_address(&mut emitter, "r11", "demo_extern");

    assert_eq!(
        emitter.output(),
        "    mov r11, QWORD PTR demo_extern@GOTPCREL[rip]\n"
    );
}

#[test]
fn test_emit_load_and_store_extern_symbol_linux_x86_64_use_shared_helpers() {
    // Verifies that load_extern_symbol_to_reg and store_reg_to_extern_symbol
    // emit the standard x86_64 sequence for accessing extern data: load the
    // symbol address via GOTPCREL into a scratch register, then dereference with
    // offset. The helpers load/store both integer (QWORD PTR) and floating-point
    // (movsd) values at arbitrary offsets.
    let mut emitter = test_emitter_x86();
    emit_load_extern_symbol_to_reg(&mut emitter, "rax", "demo_extern", 0);
    emit_load_extern_symbol_to_reg(&mut emitter, "xmm0", "demo_extern", 8);
    emit_store_reg_to_extern_symbol(&mut emitter, "r10", "demo_extern", 0);
    emit_store_reg_to_extern_symbol(&mut emitter, "xmm1", "demo_extern", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    mov r11, QWORD PTR demo_extern@GOTPCREL[rip]\n",
            "    mov rax, QWORD PTR [r11]\n",
            "    mov r11, QWORD PTR demo_extern@GOTPCREL[rip]\n",
            "    movsd xmm0, QWORD PTR [r11 + 8]\n",
            "    mov r11, QWORD PTR demo_extern@GOTPCREL[rip]\n",
            "    mov QWORD PTR [r11], r10\n",
            "    mov r11, QWORD PTR demo_extern@GOTPCREL[rip]\n",
            "    movsd QWORD PTR [r11 + 8], xmm1\n",
        )
    );
}

#[test]
fn test_emit_store_zero_to_symbol_uses_native_zero_store_on_linux_x86_64() {
    // Verifies that emit_store_zero_to_symbol uses RIP-relative LEA to get the
    // symbol address, then emits a "mov QWORD PTR [reg + offset], 0" to store
    // zero at that location. This is the standard x86_64 sequence for storing
    // an immediate zero to a global variable.
    let mut emitter = test_emitter_x86();
    emit_store_zero_to_symbol(&mut emitter, "_demo_symbol", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    lea r11, [rip + _demo_symbol]\n",
            "    mov QWORD PTR [r11 + 8], 0\n",
        )
    );
}

#[test]
fn test_emit_branch_helpers_use_native_zero_checks_on_linux_x86_64() {
    // Verifies that emit_branch_if_int_result_zero emits "test rax, rax / je label"
    // to branch when the integer in rax is zero; emit_branch_if_int_result_nonzero
    // emits "test rax, rax / jne label". The test rax, rax idiom sets the ZF flag
    // based on the register value without modifying it, which is the standard
    // zero/nonzero check pattern on x86_64.
    let mut emitter = test_emitter_x86();
    emit_branch_if_int_result_zero(&mut emitter, "zero_label");
    emit_branch_if_int_result_nonzero(&mut emitter, "nonzero_label");

    assert_eq!(
        emitter.output(),
        concat!(
            "    test rax, rax\n",
            "    je zero_label\n",
            "    test rax, rax\n",
            "    jne nonzero_label\n",
        )
    );
}

#[test]
fn test_emit_store_and_load_result_to_symbol_for_string_linux_x86_64() {
    // Verifies that emit_store_result_to_symbol stores a string result (pointer
    // in rax, length in rdx) via RIP-relative mov, and emit_load_symbol_to_result
    // loads it back. For strings, the result is stored as two adjacent QWORDs
    // at the symbol: [symbol] = pointer, [symbol + 8] = length.
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
    // Verifies that process-entry helpers emit correct x86_64 instructions:
    // emit_store_process_args_to_globals stores argc (rdi) and argv (rsi) to
    // global symbols; emit_enable_heap_debug_flag sets the heap debug flag;
    // emit_copy_frame_pointer copies rbp to a destination register; and
    // emit_exit emits the exit syscall (syscall with eax=60, edi=exit_code).
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
    // Verifies that emit_store_incoming_param emits correct instructions for
    // each SysV AMD64 argument type and position: integer arguments come from
    // rdi/rsi/rdx/rcx/r8/r9, floating-point arguments from xmm0-xmm7, and
    // arguments beyond the register arguments come from the caller stack at
    // [rbp + 16]. The cursor tracks register vs. stack parameters and advances
    // after each call.
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
    // Verifies that call and temporary-stack helpers emit correct x86_64 code:
    // emit_push_reg / emit_pop_reg handle general-purpose register save/restore;
    // emit_push_float_reg / emit_pop_float_reg handle XMM register save/restore;
    // emit_push_reg_pair / emit_pop_reg_pair handle register pair (two QWORDs);
    // emit_reserve_temporary_stack / emit_release_temporary_stack manage a
    // temporary region of the stack with sub rsp / add rsp; emit_temporary_stack_address
    // computes the address of a slot within that region; emit_load_temporary_stack_slot
    // loads from it; emit_call_label emits a direct call; emit_call_reg emits an
    // indirect call via register; and emit_store_zero_to_local_slot stores zero
    // to a frame-local slot.
    let mut emitter = test_emitter_x86();

    emit_push_reg(&mut emitter, "r12");
    super::calls::emit_pop_reg(&mut emitter, "r12");
    super::calls::emit_push_float_reg(&mut emitter, "xmm3");
    emit_pop_float_reg(&mut emitter, "xmm3");
    super::calls::emit_push_reg_pair(&mut emitter, "rax", "rdx");
    emit_pop_reg_pair(&mut emitter, "rax", "rdx");
    emit_reserve_temporary_stack(&mut emitter, 32);
    emit_temporary_stack_address(&mut emitter, "r10", 16);
    emit_load_temporary_stack_slot(&mut emitter, "r11", 24);
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
            "    movsd xmm3, QWORD PTR [rsp]\n",
            "    add rsp, 16\n",
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], rax\n",
            "    mov QWORD PTR [rsp + 8], rdx\n",
            "    mov rax, QWORD PTR [rsp]\n",
            "    mov rdx, QWORD PTR [rsp + 8]\n",
            "    add rsp, 16\n",
            "    sub rsp, 32\n",
            "    lea r10, [rsp + 16]\n",
            "    mov r11, QWORD PTR [rsp + 24]\n",
            "    call _fn_demo\n",
            "    call r12\n",
            "    add rsp, 32\n",
            "    mov QWORD PTR [rbp - 24], 0\n",
        )
    );
}

#[test]
fn test_emit_push_result_value_linux_x86_64_uses_native_result_registers() {
    // Verifies that emit_push_result_value emits the correct x86_64 instructions
    // to push a return value onto the stack for a callee-saved register fixup:
    // PhpType::Int uses rax (mov QWORD PTR [rsp], rax); PhpType::Float uses
    // xmm0 (movsd QWORD PTR [rsp], xmm0); PhpType::Str uses rax+rdx for
    // pointer and length (two QWORDs on the stack).
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

#[test]
fn test_emit_write_stdout_linux_x86_64_uses_syscall_registers() {
    // Verifies that emit_write_stdout emits the correct x86_64 instructions to
    // write to stdout via the Linux syscall interface: the integer argument is
    // converted via __rt_itoa (leaving result in rax for pointer, rdx for length),
    // then the syscall is invoked with sys_write parameters (edi=fd=1, esi=buf,
    // edx=len, eax=syscall_number=1). The function sets up the syscall registers
    // according to the Linux x86_64 syscall ABI.
    let mut emitter = test_emitter_x86();

    emit_write_stdout(&mut emitter, &PhpType::Int);

    assert_eq!(
        emitter.output(),
        concat!(
            "    call __rt_itoa\n",
            "    mov rsi, rax\n",
            "    mov rdx, rdx\n",
            "    mov edi, 1\n",
            "    mov eax, 1\n",
            "    syscall\n",
        )
    );
}
