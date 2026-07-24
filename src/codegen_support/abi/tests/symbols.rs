//! Purpose:
//! Contains ABI regression tests for symbols helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen_support::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

/// Verifies `emit_symbol_address` uses platform-appropriate relocations (ADRP + ADD
/// with @PAGE/@PAGEOFF) rather than raw immediates on ARM64.
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

/// Checks that `emit_store_result_to_symbol` stores both registers of a string (ptr in x1,
/// len in x2) at the symbol address, and that `emit_load_symbol_to_result` reverses the
/// operation correctly. Verifies str/ldr pair for x1 and x2.
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

/// Verifies `emit_extern_symbol_address` on ARM64 emits GOT-relative relocations
/// (ADRP + ldr via @GOTPAGE/@GOTPAGEOFF) rather than direct symbol addressing.
#[test]
fn test_emit_extern_symbol_address_uses_got_relocations_on_aarch64() {
    let mut emitter = test_emitter();
    crate::codegen_support::abi::symbols::emit_extern_symbol_address(&mut emitter, "x9", "_demo_extern");

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_extern@GOTPAGE\n",
            "    ldr x9, [x9, _demo_extern@GOTPAGEOFF]\n",
        )
    );
}

/// Verifies `emit_extern_symbol_address` on windows-x86_64 emits a GOT-free,
/// direct RIP-relative LEA instead of the ELF-style `@GOTPCREL` load: PE/COFF
/// has no GOTPCREL relocation, and MinGW's assembler treats `@GOTPCREL` as a
/// literal name suffix rather than a relocation, producing an undefined
/// reference at link time. These "extern" symbols resolve against a
/// statically-linked `.a` archive built into the same image rather than a
/// DLL, so no `__imp_` indirection is needed either.
#[test]
fn test_emit_extern_symbol_address_uses_rip_relative_lea_on_windows_x86_64() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::abi::symbols::emit_extern_symbol_address(&mut emitter, "r9", "demo_extern");
    let asm = emitter.output();

    assert_eq!(asm, "    lea r9, [rip + demo_extern]\n");
    assert!(
        !asm.contains("GOTPCREL"),
        "windows PE/COFF must never emit @GOTPCREL (MinGW treats it as a literal suffix, not a relocation)"
    );
}

/// Verifies `emit_extern_symbol_address` on linux-x86_64 stays byte-identical
/// to before the windows-x86_64 GOT-free LEA arm was added: it must still
/// emit the exact ELF `@GOTPCREL[rip]` form.
#[test]
fn test_emit_extern_symbol_address_stays_gotpcrel_on_linux_x86_64_after_windows_fix() {
    let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
    crate::codegen_support::abi::symbols::emit_extern_symbol_address(&mut emitter, "r9", "demo_extern");

    assert_eq!(
        emitter.output(),
        "    mov r9, QWORD PTR demo_extern@GOTPCREL[rip]\n"
    );
}
