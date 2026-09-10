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

/// Checks that string result stores and loads use the AArch64 symbol scratch.
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

/// Verifies that a non-PIC AArch64 floating-point symbol load uses x9 as its
/// documented address scratch without changing the stack pointer.
#[test]
fn test_non_pic_float_load_uses_x9_scratch_on_aarch64() {
    let mut emitter = test_emitter();
    emit_load_symbol_to_reg(&mut emitter, "d0", "_demo_symbol", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_symbol@PAGE\n",
            "    add x9, x9, _demo_symbol@PAGEOFF\n",
            "    ldr d0, [x9, #8]\n",
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

/// Verifies that a PIC AArch64 floating-point symbol load uses x9 while
/// resolving the external address through the GOT.
#[test]
fn test_pic_float_load_uses_x9_scratch_on_aarch64() {
    let mut emitter = Emitter::new_pic(Target::new(Platform::MacOS, Arch::AArch64));
    emit_load_symbol_to_reg(&mut emitter, "d0", "_demo_extern", 8);

    assert_eq!(
        emitter.output(),
        concat!(
            "    adrp x9, _demo_extern@GOTPAGE\n",
            "    ldr x9, [x9, _demo_extern@GOTPAGEOFF]\n",
            "    ldr d0, [x9, #8]\n",
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

/// Gates the callable-descriptor static store on every AArch64 supported target
/// (macos-aarch64, ios-arm64, ios-sim-arm64, linux-aarch64): the replacement descriptor must be
/// published into the slot before the previous descriptor is released, the previous descriptor
/// must be captured before the publish overwrites it, and the descriptor result must be preserved
/// across a single 16-byte aligned push/pop bracketing the release call so the nested call stays
/// aligned even when a captured-object destructor throws or reenters the slot.
#[test]
fn test_callable_static_store_publishes_before_release_aarch64() {
    use crate::codegen_support::platform::AppleVariant;
    let targets = [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
        Target::new(Platform::Linux, Arch::AArch64),
    ];
    for target in targets {
        let mut emitter = Emitter::new(target);
        emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Callable, true);
        let out = emitter.output();

        let load_prev = out
            .find("    ldr x10, [x9]\n")
            .unwrap_or_else(|| panic!("previous descriptor load missing on {}", target));
        let publish = out
            .find("    str x0, [x9]\n")
            .unwrap_or_else(|| panic!("published descriptor store missing on {}", target));
        let push = out
            .find("    str x0, [sp, #-16]!\n")
            .unwrap_or_else(|| panic!("aligned descriptor preserve missing on {}", target));
        let release = out
            .find("    bl __rt_callable_descriptor_release\n")
            .unwrap_or_else(|| panic!("descriptor release call missing on {}", target));
        let pop = out
            .find("    ldr x0, [sp], #16\n")
            .unwrap_or_else(|| panic!("aligned descriptor restore missing on {}", target));

        assert!(load_prev < publish, "previous descriptor must be captured before publish on {}", target);
        assert!(publish < push, "descriptor must be published before it is preserved on {}", target);
        assert!(push < release, "descriptor result must be preserved before the release call on {}", target);
        assert!(release < pop, "descriptor result must be restored after the release call on {}", target);
        assert!(out.contains("    mov x0, x10\n"), "release argument move missing on {}", target);
    }
}

/// Gates the callable-descriptor static store on linux-x86_64: same publish-before-retire ordering
/// and a single 16-byte aligned `sub rsp, 16` / `add rsp, 16` bracket around the SysV release call.
#[test]
fn test_callable_static_store_publishes_before_release_x86_64() {
    let mut emitter = test_emitter_x86();
    emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Callable, true);
    let out = emitter.output();

    let load_prev = out
        .find("    mov r10, QWORD PTR [rip + _demo_symbol]\n")
        .expect("previous descriptor load missing on linux-x86_64");
    let publish = out
        .find("    mov QWORD PTR [rip + _demo_symbol], rax\n")
        .expect("published descriptor store missing on linux-x86_64");
    let push = out
        .find("    sub rsp, 16\n")
        .expect("aligned descriptor preserve missing on linux-x86_64");
    let release = out
        .find("    call __rt_callable_descriptor_release\n")
        .expect("descriptor release call missing on linux-x86_64");
    let pop = out
        .find("    add rsp, 16\n")
        .expect("aligned descriptor restore missing on linux-x86_64");

    assert!(load_prev < publish, "previous descriptor must be captured before publish");
    assert!(publish < push, "descriptor must be published before it is preserved");
    assert!(push < release, "descriptor result must be preserved before the release call");
    assert!(release < pop, "descriptor result must be restored after the release call");
    assert!(out.contains("    mov rax, r10\n"), "release argument move missing on linux-x86_64");
}

/// Verifies the first callable-descriptor static store (no previous owner) publishes the
/// descriptor without emitting any release call or stack bracket.
#[test]
fn test_callable_static_store_without_previous_owner_only_publishes() {
    let mut emitter = test_emitter();
    emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Callable, false);
    let out = emitter.output();

    assert!(out.contains("    str x0, [x9]\n"), "descriptor must be published into the slot");
    assert!(
        !out.contains("__rt_callable_descriptor_release"),
        "no descriptor release may run when there is no previous owner"
    );
    assert!(
        !out.contains("[sp, #-16]!"),
        "no stack bracket is needed when there is no release call"
    );
}
