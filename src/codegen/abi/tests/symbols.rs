//! Purpose:
//! Contains ABI regression tests for symbols helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

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
fn test_emit_extern_symbol_address_uses_got_relocations_on_aarch64() {
    let mut emitter = test_emitter();
    crate::codegen::abi::symbols::emit_extern_symbol_address(&mut emitter, "x9", "_demo_extern");

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_extern@GOTPAGE\n",
            "    ldr x9, [x9, _demo_extern@GOTPAGEOFF]\n",
        )
    );
}
