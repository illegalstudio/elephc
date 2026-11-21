//! Purpose:
//! Separates guarded property storage from promoted constructor reference bindings.
//!
//! Called from:
//! - Focused all-target and native strict Reflection regressions.
//!
//! Key details:
//! - Reflection default machinery must compile alongside ordinary subclass properties.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/reflection_guarded_properties.php");

/// Guarded property cells do not force speculative reference constructors on any target.
#[test]
fn reflection_guarded_properties_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for coercions in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, coercions);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            let signature = &module.class_infos["GuardedReflectionOwner"].methods["__construct"];
            assert!(signature.ref_params.iter().all(|by_ref| !by_ref));
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Scalar parameter defaults and guarded by-value properties retain their values and owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reflection_guarded_properties_native_preserve_defaults_and_owners() {
    let reflection_source = SOURCE.split("class GuardedByValueBox").next().unwrap();
    let owned_source = SOURCE.replace("'GuardedByValueBox', 'vv'", "'GuardedByValueBox', str_repeat('v', 2)");
    for coercions in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let sources = if coercions && mode != SourceMode::Internal {
                    vec![SOURCE, owned_source.as_str()]
                } else { vec![reflection_source] };
                for source in sources {
                    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, coercions);
                    let module = super::semantic_sources::lower(source, Target::detect_host(), profile, optimize, mode);
                    let output = super::native_module::run(&module);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert!(output.status.success(), "{:?}: {:?}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
                    let expected = if coercions && mode != SourceMode::Internal {
                        "GuardedReflectionSubject:tag|ok|Bvv|K|"
                    } else { "GuardedReflectionSubject:tag|ok|" };
                    assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "coercions={coercions}, mode={mode:?}: {stderr}");
                    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "coercions={coercions}, optimize={optimize}, mode={mode:?}: {stderr}");
                }
            }
        }
    }
}
