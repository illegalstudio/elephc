//! Purpose:
//! Verifies target-aware guards in specialized mbstring count lowering.
//!
//! Called from:
//! - The AST-to-EIR integration unit suite.
//!
//! Key details:
//! - Every supported target emits the live request guard and count failure branch.

/// Length-only regex specialization keeps state and invalid-result guards on every target.
#[test]
fn mb_fast_semantics_count_guards_cover_all_targets() {
    let source = r#"<?php
$s = $argc > 0 ? "a@b" : "";
echo count(mb_split("@", $s));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        crate::ir_passes::optimize_module(&mut module);
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("elephc_mbstring_regex_ascii_defaults_v1"), "{target}: live state guard");
        assert!(asm.contains("mb_split_count_array"), "{target}: false-result count guard");
    }
}
