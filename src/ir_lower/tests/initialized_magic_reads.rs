//! Purpose:
//! Verifies initialized magic-property reads for concrete and boxed receivers.
//!
//! Called from:
//! - Focused staged initialized-read tests.
//!
//! Key details:
//! - Removed untyped slots preserve getter and intentional probe protocols.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/initialized_magic_reads.php");
const EXPECTED: &str = "ready:1|PF:ready:3|ready:1|PF:ready|3|NULL\n0|ready:4|T|null:null|ready:ready|6|G7|AE:G7|child:1|";

/// Every supported target lowers the same initialized magic dispatch.
#[test]
fn initialized_magic_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Getters and quiet probes preserve values, call counts and temporary owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_magic_reads_native_values_and_owners() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)
            .with(StrictFeature::Locals, locals);
        for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
            (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
            let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "opt={optimize}: stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}

/// Quiet container reads select the getter directly and retain the existing shape refusal.
#[test]
fn initialized_magic_quiet_reads_use_getter() {
    let source = r#"<?php $name = $argc > 0 ? 'value' : 'other';
class InitializedMagicUnsetContainer {
    public $value;
    public function __get(string $name): mixed { echo 'U'; return ['x' => 1]; }
    public function __isset(string $name): bool { echo 'bad'; return false; }
}
function initializedMagicUnsetDimension(mixed $object, string $name): void {
    unset($object->$name['x']);
}
$unsetMagic = new InitializedMagicUnsetContainer();
unset($unsetMagic->value);
initializedMagicUnsetDimension($unsetMagic, $name);
echo 'Q|';
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, strict);
            let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            let function = module.functions.iter().find(|function| function.name == "initializedMagicUnsetDimension").unwrap();
            if strict {
                let methods = function.instructions.iter().filter_map(|inst| {
                    if inst.op != crate::ir::Op::MethodCall { return None; }
                    let data = match inst.immediate {
                        Some(crate::ir::Immediate::Data(data) | crate::ir::Immediate::InitializedGetter { method: data, .. }) => data,
                        _ => return None,
                    };
                    Some(module.data.strings[data.as_raw() as usize].as_str())
                }).collect::<Vec<_>>();
                assert!(methods.contains(&"__get"), "{name}: {methods:?}");
                assert!(!methods.contains(&"__isset"), "{name}: {methods:?}");
            }
            let error = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap_err();
            assert!(error.to_string().contains("runtime-dependent magic property shapes"), "{name}: {error}");
        }
    }
}
