//! Purpose:
//! Checks regex output references and context-preserving iteration on every target.
//!
//! Called from:
//! - The AST-to-EIR regression test module.
//!
//! Key details:
//! - Ref-cell loads must select capture emission rather than silently count only.
//! - Runtime iteration submits an original subject plus a range to the opaque C shim.

/// Checks capture writeback and runtime range setup for one supported target.
fn verify(target: &str) {
    let target = crate::codegen_support::platform::Target::parse(target).unwrap();
    let module = super::lower_source_at_for_target(r#"<?php
function collect(array &$matches): int {
    return preg_match_all('/a|(?<=a)b/', 'ab', $matches, PREG_OFFSET_CAPTURE);
}
$matches = []; echo collect($matches);
"#, std::path::Path::new("main.php"), std::path::Path::new("."), target);
    let function = module.functions.iter().find(|function| function.name == "collect").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::LoadRefCell));
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("__rt_preg_match_all_capture"));
    assert!(assembly.contains("__rt_decref_array"), "the replaced output owns its old captures");
}

/// Keeps every target independently scheduled under the CI timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Checks capture references and iteration offsets for this target.
        #[test]
        fn $name() { verify($target); }
    };
}

target_test!(preg_match_all_review_macos, "macos-aarch64");
target_test!(preg_match_all_review_ios, "ios-arm64");
target_test!(preg_match_all_review_ios_sim, "ios-sim-arm64");
target_test!(preg_match_all_review_linux_arm, "linux-aarch64");
target_test!(preg_match_all_review_linux_x86, "linux-x86_64");
