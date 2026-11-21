//! Purpose:
//! Verifies typed numeric-string names through shared call and unpack plans.
//!
//! Called from:
//! - Focused all-target EIR and native ownership regressions.
//!
//! Key details:
//! - Direct, callable and Reflection unpack paths preserve key kind and byte identity.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};
use super::semantic_sources::lower as lower_profile;

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/static_unpack_keys.php");

/// Lowers the actual selected frontend and optionally runs the complete AST/EIR optimizer.
fn lower(target: Target, strict: bool, optimize: bool, mode: SourceMode) -> crate::ir::Module {
    lower_source(SOURCE, target, strict, optimize, mode)
}

/// Keeps isolated ownership probes on the same frontend and lowering path as the fixture.
fn lower_source(source: &str, target: Target, strict: bool, optimize: bool, mode: SourceMode) -> crate::ir::Module {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
    lower_profile(source, target, profile, optimize, mode)
}

/// Profiled unpack graphs validate and emit on every supported target.
#[test]
fn static_unpack_keys_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::parse(name).unwrap(), strict, false, mode);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Native unpacking preserves numeric names, NUL bytes and separate-spread append order.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn static_unpack_keys_native_preserve_numeric_names() {
    for strict in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::detect_host(), strict, optimize, mode);
                let output = super::native_module::run(&module);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "strict={strict}, opt={optimize}: {stdout} {stderr}");
                let expected = if strict {
                    "string:0=10,|integer:0=1,string:0=2,|string:0=10,string:01=20,|string:0=30,|string:0=40,|string:0=50,|string:0=10,integer:0=20,|integer:0=20,string:0=10,|string:a\0b=60,|double:2|"
                } else {
                    "integer:0=10,|integer:0=2,|integer:0=10,string:01=20,|integer:0=30,|integer:0=40,|integer:0=50,|integer:0=10,integer:1=20,|integer:0=20,integer:1=10,|string:a\0b=60,|double:2|"
                };
                assert_eq!(stdout, expected, "strict={strict}, opt={optimize}, mode={mode:?}: {stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}


/// Every opaque unpack source retires its independent owner after a boxed collector return.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn static_unpack_isolated_calls_retire_opaque_hash_sources() {
    let prefix = SOURCE.split("dumpUnpackKeys(gatherUnpackKeys").next().unwrap();
    let mut failed = false;
    for (name, body) in [
        ("empty", ""),
        ("direct", "dumpUnpackKeys(gatherUnpackKeys(...['0' => 10]));"),
        ("duplicate", "dumpUnpackKeys(gatherUnpackKeys(...[0 => 1, '0' => 2]));"),
        ("runtime", "$values = ['0' => 10, '01' => 20]; dumpUnpackKeys(gatherUnpackKeys(...$values));"),
        ("callback", "$callback = gatherUnpackKeys(...); dumpUnpackKeys($callback(...['0' => 30]));"),
        ("constructor", "$object = new NumericUnpackConstructor(...['0' => 40]); dumpUnpackKeys($object->values);"),
        ("reflection", "$r = new ReflectionClass('NumericUnpackConstructor'); $o = $r->newInstanceArgs(['0' => 50]); dumpUnpackKeys($o->values);"),
        ("cross", "dumpUnpackKeys(gatherUnpackKeys(...[20], ...['0' => 10]));"),
    ] {
        let module = lower_source(&format!("{prefix}\n{body}"), Target::detect_host(), false, false, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("{name}: {:?}; {}", String::from_utf8_lossy(&output.stdout),
            stderr.lines().find(|line| line.contains("leak summary")).unwrap_or(&stderr));
        failed |= !output.status.success() || !stderr.contains("HEAP DEBUG: leak summary: clean");
    }
    assert!(!failed);
}


/// Reflection constructor reuse preserves independently selected conversion checks.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reflected_constructor_raw_args_keep_conversion_contracts() {
    let source = r#"<?php
function opaqueConstructorValue(mixed $value): mixed { return $value; }
class RawConstructorFloat { public float $value; public function __construct(float $value) { $this->value = $value; } }
$reflection = new ReflectionClass('RawConstructorFloat');
try { $object = $reflection->newInstanceArgs(['value' => opaqueConstructorValue(2)]); echo gettype($object->value); }
catch (TypeError $error) { echo 'K'; }
"#;
    for keys in [false, true] {
        for coercions in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys)
                .with(StrictFeature::Coercions, coercions);
            let module = lower_profile(source, Target::detect_host(), profile, false, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), if coercions { "K" } else { "double" }, "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
