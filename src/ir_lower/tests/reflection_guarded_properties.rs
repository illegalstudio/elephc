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
fn verify_reflection_guarded_properties_native(coercions: bool, optimize: bool) {
    let reflection_source = SOURCE.split("class GuardedByValueBox").next().unwrap();
    let owned_source = SOURCE.replace("'GuardedByValueBox', 'vv'", "'GuardedByValueBox', str_repeat('v', 2)");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, coercions);
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let (sources, expected) = if coercions && mode != SourceMode::Internal {
            (vec![SOURCE, owned_source.as_str()], "GuardedReflectionSubject:tag|ok|Bvv|K|")
        } else { (vec![reflection_source], "GuardedReflectionSubject:tag|ok|") };
        for source in sources {
            let module = super::semantic_sources::lower(source, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{:?}: {:?}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "coercions={coercions}, mode={mode:?}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "coercions={coercions}, optimize={optimize}, mode={mode:?}: {stderr}");
        }
    }
}

#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
mod native_profiles {
    use super::verify_reflection_guarded_properties_native;

    /// Runs the property and default checks without strict coercions or optimization.
    #[test]
    fn permissive_unoptimized() { verify_reflection_guarded_properties_native(false, false); }

    /// Runs the property and default checks without strict coercions, with optimization.
    #[test]
    fn permissive_optimized() { verify_reflection_guarded_properties_native(false, true); }

    /// Runs the strict coercion property and default checks without optimization.
    #[test]
    fn strict_unoptimized() { verify_reflection_guarded_properties_native(true, false); }

    /// Runs the strict coercion property and default checks with optimization.
    #[test]
    fn strict_optimized() { verify_reflection_guarded_properties_native(true, true); }
}
