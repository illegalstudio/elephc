//! Purpose:
//! Verifies Initialization across boxed receivers, union views and shared property dispatch.
//!
//! Called from:
//! - Focused staged initialized-read tests.
//!
//! Key details:
//! - Repeated lexical sites exercise shared helpers with distinct read and probe policies.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/initialized_boxed_reads.php");
const EXPECTED: &str = "NULL:NULL|I|I|AE:missing|AE:missing|I|I|I|NULL|I|AE:missing|I|I|";

/// Every supported target lowers the same scope and property initialization boundaries.
#[test]
fn initialized_boxed_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Missing source storage throws while intentional nested probes remain silent.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_boxed_reads_native_values_and_owners() {
    for (initialization, locals) in [(true, false), (true, true), (false, false)] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, initialization)
            .with(StrictFeature::Locals, locals);
        for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
            (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
            let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "opt={optimize}: stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), if initialization { EXPECTED } else { "NULL:NULL|bad|bad|AE:missing|AE:missing|bad|bad|bad|NULL|bad|AE:missing|bad|bad|" }, "init={initialization}, locals={locals}, opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}
