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

/// Checks that string result stores use a separate scratch while loads reuse
/// each destination register for address materialization without clobbering x9.
#[test]
fn test_emit_store_and_load_result_to_symbol_for_string() {
    let mut emitter = test_emitter();
    emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Str, false);
    emit_load_symbol_to_result(&mut emitter, "_demo_symbol", &PhpType::Str);
    let out = emitter.output();

    assert!(out.contains("    str x1, [x9]\n"));
    assert!(out.contains("    str x2, [x9, #8]\n"));
    assert!(out.contains("    adrp x1, _demo_symbol@PAGE\n"));
    assert!(out.contains("    ldr x1, [x1]\n"));
    assert!(out.contains("    adrp x2, _demo_symbol@PAGE\n"));
    assert!(out.contains("    ldr x2, [x2, #8]\n"));
}

/// Verifies that an x9 destination survives address materialization by serving
/// as its own address scratch for a non-PIC AArch64 symbol load.
#[test]
fn test_non_pic_load_x9_self_scratches_on_aarch64() {
    let mut emitter = test_emitter();
    emit_load_symbol_to_reg(&mut emitter, "x9", "_demo_symbol", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_symbol@PAGE\n",
            "    add x9, x9, _demo_symbol@PAGEOFF\n",
            "    ldr x9, [x9, #8]\n",
        )
    );
}

/// Verifies that a non-PIC AArch64 floating-point symbol load preserves x9
/// behind a 16-byte stack slot while using it for the address.
#[test]
fn test_non_pic_float_load_preserves_x9_on_aarch64() {
    let mut emitter = test_emitter();
    emit_load_symbol_to_reg(&mut emitter, "d0", "_demo_symbol", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    sub sp, sp, #16\n",
            "    str x9, [sp]\n",
            "    adrp x9, _demo_symbol@PAGE\n",
            "    add x9, x9, _demo_symbol@PAGEOFF\n",
            "    ldr d0, [x9, #8]\n",
            "    ldr x9, [sp]\n",
            "    add sp, sp, #16\n",
        )
    );
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

/// Preserves an x9 payload by resolving a PIC symbol address through x10 before storing.
#[test]
fn test_pic_store_x9_uses_distinct_aarch64_got_scratch() {
    let mut emitter = Emitter::new_pic(Target::new(Platform::MacOS, Arch::AArch64));
    emit_store_reg_to_symbol(&mut emitter, "x9", "_demo_extern", 0);

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x10, _demo_extern@GOTPAGE\n",
            "    ldr x10, [x10, _demo_extern@GOTPAGEOFF]\n",
            "    str x9, [x10]\n",
        )
    );
}

/// Verifies that a PIC AArch64 integer symbol load resolves the GOT through
/// the destination itself, including when the requested destination is x9.
#[test]
fn test_pic_load_x9_self_scratches_on_aarch64() {
    let mut emitter = Emitter::new_pic(Target::new(Platform::MacOS, Arch::AArch64));
    emit_load_symbol_to_reg(&mut emitter, "x9", "_demo_extern", 0);

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_extern@GOTPAGE\n",
            "    ldr x9, [x9, _demo_extern@GOTPAGEOFF]\n",
            "    ldr x9, [x9]\n",
        )
    );
}

/// Verifies that a PIC AArch64 floating-point symbol load preserves x9 while
/// resolving the external address through the GOT.
#[test]
fn test_pic_float_load_preserves_x9_on_aarch64() {
    let mut emitter = Emitter::new_pic(Target::new(Platform::MacOS, Arch::AArch64));
    emit_load_symbol_to_reg(&mut emitter, "d0", "_demo_extern", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    sub sp, sp, #16\n",
            "    str x9, [sp]\n",
            "    adrp x9, _demo_extern@GOTPAGE\n",
            "    ldr x9, [x9, _demo_extern@GOTPAGEOFF]\n",
            "    ldr d0, [x9, #8]\n",
            "    ldr x9, [sp]\n",
            "    add sp, sp, #16\n",
        )
    );
}

/// Preserves an x9 payload through a non-PIC AArch64 symbol-address calculation.
#[test]
fn test_non_pic_store_x9_uses_distinct_aarch64_scratch() {
    let mut emitter = test_emitter();
    emit_store_reg_to_symbol(&mut emitter, "x9", "_demo_symbol", 0);

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x10, _demo_symbol@PAGE\n",
            "    add x10, x10, _demo_symbol@PAGEOFF\n",
            "    str x9, [x10]\n",
        )
    );
}
