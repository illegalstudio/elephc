//! Purpose:
//! Checks original builtin keys through reflection and named argument arrays.
//!
//! Called from:
//! - Focused staged compiler and all-target regressions.
//!
//! Key details:
//! - Argument arrays retain original key values until prepared builtin validation.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds reflected key observations without exposing the argument kind to the callee.
fn source(exact: bool) -> (String, String) {
    let mut source = include_str!("../../../tests/fixtures/strict/builtin_key_reflection.php").to_string();
    let mut expected = String::new();
    for (argument, invalid) in [("true", true), ("0.0", true), ("null", true),
        ("0", false), ("'0'", false), ("''", false)] {
        source.push_str(&format!("reflectedKeyInputs({argument});\n"));
        expected.push_str(if exact && invalid { "R|A|C|T|V|" } else { "r|a|c|t|v|" });
        expected.push_str(if exact { "4|p|q|" } else { "3|p|q|" });
    }
    (source, expected)
}

/// Reflected builtin and user-method arguments keep exact source kinds on every target.
#[test]
fn builtin_key_reflection_emit_on_every_target() {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(),
            profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Strict key consumers diagnose unresolved Reflection identities after exception joins.
#[test]
fn builtin_key_reflection_try_joins_diagnose_unknown_targets() {
    let declarations = "<?php function reflectionBefore(): int { return 11; } \
        function reflectionAfter(): int { return 22; } \
        function replaceReflection(&$ref): void { $ref = new ReflectionFunction('reflectionAfter'); } \
        class UnknownRefMethod { public static function run(): int { return 1; } } ";
    let cases = [
        ("function probe(): mixed { $ref = new ReflectionFunction('reflectionBefore'); \
            try { echo ''; } finally { echo ''; } return $ref->invoke(); }", "ReflectionFunction"),
        ("function probe(): mixed { $ref = new ReflectionFunction('reflectionBefore'); \
            try { $ref = new ReflectionFunction('reflectionAfter'); } catch (TypeError $e) {} \
            return $ref->invoke(); }", "ReflectionFunction"),
        ("function probe(): mixed { $ref = new ReflectionFunction('reflectionBefore'); \
            try { replaceReflection($ref); } catch (TypeError $e) {} return $ref->invoke(); }", "ReflectionFunction"),
        ("function probe(): mixed { $ref = new ReflectionFunction('reflectionBefore'); \
            $replace = function () use (&$ref): void { $ref = new ReflectionFunction('reflectionAfter'); }; \
            try { $replace(); } catch (TypeError $e) {} return $ref->invoke(); }", "ReflectionFunction"),
        ("function probe(ReflectionFunction $ref): mixed { return $ref->invokeArgs([]); }", "ReflectionFunction"),
        ("function probe(): mixed { $ref = new ReflectionMethod(UnknownRefMethod::class, 'run'); \
            try { echo ''; } finally { echo ''; } return $ref->invoke(null); }", "ReflectionMethod"),
    ];
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for (body, class) in cases {
            let source = format!("{declarations}{body}");
            let result = super::semantic_sources::try_lower_capturing_arguments(&source, Target::parse(target).unwrap(),
                SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true), false, SourceMode::Php);
            match result {
                Err(error) => {
                    let message = error.to_string();
                    assert!(message.contains("--strict-array-keys") && message.contains(class), "{target}: {message}");
                }
                Ok(module) => {
                    let function = module.functions.iter().find(|function| function.name == "probe").unwrap();
                    assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::ThrowError
                        && matches!(inst.immediate, Some(crate::ir::Immediate::Data(id))
                            if module.data.strings[id.as_raw() as usize].contains("--strict-array-keys")
                                && module.data.strings[id.as_raw() as usize].contains(class))),
                        "{target}: missing guard for boxed {class} receiver: {body}");
                    crate::ir::validate_module(&module).unwrap();
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}

/// Reflection and named-argument adapters preserve original key guards and array owners natively.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn builtin_key_reflection_native_values_and_owners() {
    for exact in [false, true] {
        let (source, expected) = source(exact);
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(),
                profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "exact={exact} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "exact={exact} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "exact={exact} opt={optimize}: {stderr}");
        }
    }
}

/// Boxed receivers reject unsupported Reflection targets while ordinary invocations retain their values.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn builtin_key_reflection_native_boxed_receivers_and_diagnostic_owners() {
    let source = include_str!("../../../tests/fixtures/strict/builtin_key_reflection_unknown.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(),
            profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "blocked|boolean:user|blocked|integer:user|",
            "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
