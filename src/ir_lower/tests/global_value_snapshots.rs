//! Purpose:
//! Checks borrowed global reads while closure snapshots outlive replacement of program storage.
//!
//! Called from:
//! - Focused strict-local checker and native global regressions.
//!
//! Key details:
//! - Program symbols retain their owner independently of each captured snapshot.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/global_value_snapshots.php");
const CHECKED: &str = "1:2|old:next|1:2|1:2|";

/// Every target borrows global payloads while a snapshot takes its own independent lease.
#[test]
fn global_value_snapshots_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        for function in &module.functions {
            for instruction in &function.instructions {
                if instruction.op == crate::ir::Op::LoadGlobal {
                    let value = function.value(instruction.result.unwrap()).unwrap();
                    if crate::ir::Ownership::php_type_needs_lifetime_tracking(&value.php_type) {
                        assert_eq!(value.ownership, crate::ir::Ownership::Borrowed, "{name}: {}", function.name);
                    }
                }
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Both profiles and optimizer modes preserve captured scalar and heap values after replacement.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_value_snapshots_native_values_and_owners() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}
