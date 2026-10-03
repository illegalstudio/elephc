//! Purpose:
//! Assembly regressions for distinct PHP Error paths when a boxed Mixed throw is invalid.
//!
//! Called from:
//! - AST-to-EIR unit tests through `crate::ir_lower::tests`.
//!
//! Key details:
//! - Both invalid branches release their owned box before allocating the catchable Error.
//! - Each supported target has a separate test to bound per-test assembly-generation work.

/// Compiles both deferred throw forms and checks their target-aware error and cleanup paths.
fn assert_mixed_throw_error_assembly(target: &str) {
    let source = r#"<?php
eval('$scalar = 42; $plain_object = new stdClass();');
try { throw $scalar; } catch (Error $error) { echo $error->getMessage(); }
try { $unused = true ? throw $plain_object : null; } catch (Error $error) { echo $error->getMessage(); }
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for marker in [
        "release a non-object thrown box before raising Error",
        "release a non-Throwable object box before raising Error",
    ] {
        let paths: Vec<_> = asm.split(marker).skip(1).collect();
        assert_eq!(paths.len(), 2, "{target}: both throw forms need {marker}");
        for path in paths {
            let release = path.find("__rt_decref_mixed").expect("release invalid operand box");
            let error_class = path.find("_spl_error_class_id").expect("allocate PHP Error, not TypeError");
            assert!(release < error_class, "{target}: cleanup precedes Error creation");
        }
    }
    assert!(asm.contains("Cannot throw objects that do not implement Throwable"), "{target}");
}

/// macOS ARM64 emits distinct, owned Error branches for invalid Mixed throws.
#[test]
fn mixed_throw_error_macos_assembly() {
    assert_mixed_throw_error_assembly("macos-aarch64");
}

/// iOS device ARM64 keeps the same deferred throw validation and cleanup.
#[test]
fn mixed_throw_error_ios_device_assembly() {
    assert_mixed_throw_error_assembly("ios-arm64");
}

/// iOS Simulator ARM64 keeps the same deferred throw validation and cleanup.
#[test]
fn mixed_throw_error_ios_simulator_assembly() {
    assert_mixed_throw_error_assembly("ios-sim-arm64");
}

/// Linux ARM64 emits both distinct PHP Error diagnostics.
#[test]
fn mixed_throw_error_linux_arm64_assembly() {
    assert_mixed_throw_error_assembly("linux-aarch64");
}

/// Linux x86_64 follows the same Error and boxed-operand cleanup contract.
#[test]
fn mixed_throw_error_linux_x86_64_assembly() {
    assert_mixed_throw_error_assembly("linux-x86_64");
}
