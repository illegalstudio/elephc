//! Purpose:
//! Checks retirement of detached values before a foreach name binds the next array element.
//!
//! Called from:
//! - Focused strict-local checker and native iterator regressions.
//!
//! Key details:
//! - Detached raw owners retire while ordinary attached references retain their source cells.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/foreach_binding_ends.php");
const CHECKED: &str = "new|new|1,2|string:new|string:new|integer:2|1,2|1|2|1,2|";

/// Every target retires detached raw storage before rebinding an iteration reference.
#[test]
fn foreach_binding_ends_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Each profile and optimizer mode preserves array values and retires detached loop owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn foreach_binding_ends_native_values_and_owners() {
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
