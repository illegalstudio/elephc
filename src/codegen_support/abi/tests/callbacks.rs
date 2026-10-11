//! Purpose:
//! Verifies target-aware native C callback entry adaptation.
//! Pins Windows MSx64 register normalization and overflow stack offsets.
//!
//! Called from:
//! - `crate::codegen_support::abi::tests` through Rust's test harness.
//!
//! Key details:
//! - Unix callback entries must remain byte-free of the Windows adapter.

use super::*;

/// Verifies Windows callback entries normalize all SysV-shaped integer inputs
/// and expose a private adapter-free label for generated assembly calls.
#[test]
fn test_windows_c_callback_entry_adapts_msx64_registers() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));

    emit_c_callback_entry(&mut emitter, "__elephc_eval_value_string_bytes");
    let output = emitter.output();

    assert!(output.contains("mov rdi, rcx"));
    assert!(output.contains("mov rsi, rdx"));
    assert!(output.contains("mov rdx, r8"));
    assert!(output.contains("mov rcx, r9"));
    assert!(output.contains("mov r8, QWORD PTR [rsp + 40]"));
    assert!(output.contains("mov r9, QWORD PTR [rsp + 48]"));
    assert!(!output.contains("mov QWORD PTR [rsp + 48], rax"));
    assert!(output.contains("movdqu XMMWORD PTR [rsp + 16], xmm6"));
    assert!(output.contains("movdqu xmm15, XMMWORD PTR [rsp + 160]"));
    assert!(output.contains("mov rdi, QWORD PTR [rsp + 0]"));
    assert!(output.contains("mov rsi, QWORD PTR [rsp + 8]"));
    assert!(output.contains("__elephc_eval_value_string_bytes__internal:"));
}

/// Verifies Unix x86_64 callback entries preserve their native SysV registers.
#[test]
fn test_linux_c_callback_entry_needs_no_register_adapter() {
    let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));

    emit_c_callback_entry(&mut emitter, "__elephc_eval_value_string_bytes");
    let output = emitter.output();

    assert!(!output.contains("mov rdi, rcx"));
    assert!(output.contains("__elephc_eval_value_string_bytes__internal:"));
}

/// Verifies Mach-O dead-strip localization does not rewrite a cross-object
/// callback alias into an assembler-local symbol that `.globl` cannot export.
#[test]
fn test_macos_c_callback_internal_alias_is_not_localized() {
    let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
    emitter.dead_strip = true;

    emit_c_callback_entry(&mut emitter, "__elephc_eval_value_string_bytes");

    assert!(emitter.take_internal_labels().is_empty());
    let output = emitter.output();
    assert!(output.contains(".globl ___elephc_eval_value_string_bytes__internal"));
    assert!(!output.contains(".globl L___elephc_eval_value_string_bytes__internal"));
}

/// Verifies high-arity Windows callbacks skip shadow space and the two stack
/// arguments promoted into the SysV-shaped r8/r9 registers.
#[test]
fn test_windows_c_callback_stack_offsets_cover_high_arity_hooks() {
    let target = Target::new(Platform::Windows, Arch::X86_64);

    assert_eq!(c_callback_stack_arg_offset(target, 6), 64);
    assert_eq!(c_callback_stack_arg_offset(target, 16), 144);
}

#[test]
fn test_windows_high_arity_entry_forwards_only_declared_overflow_arguments() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_c_callback_entry_with_arity(&mut emitter, "__elephc_eval_reflection_owner_new", 17);
    let output = emitter.output();

    assert_eq!(output.matches("mov rax, QWORD PTR [rsp +").count(), 11);
    assert!(output.contains("mov QWORD PTR [rsp + 48], rax"));
    assert!(output.contains("mov QWORD PTR [rsp + 128], rax"));
    assert!(!output.contains("mov QWORD PTR [rsp + 136], rax"));
    assert!(output.contains("sub rsp, 312"));
}

/// Verifies SysV callback overflow arguments retain their original frame offsets.
#[test]
fn test_linux_c_callback_stack_offsets_remain_sysv() {
    let target = Target::new(Platform::Linux, Arch::X86_64);

    assert_eq!(c_callback_stack_arg_offset(target, 6), 16);
    assert_eq!(c_callback_stack_arg_offset(target, 16), 96);
}
