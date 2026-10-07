//! Purpose:
//! Checks strip_tags array normalization and temporary owners on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR regression tests.
//!
//! Key details:
//! - A boxed allow-list must use checked dense values, not the hash payload directly.
//! - Normalized array owners retire after the join and before the string result returns.

/// Generates one target's boxed array allow-list path and checks its conversion boundary.
fn verify(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function stripWith(mixed $allow): string {
    return strip_tags('<p>A</p><b>B</b>', $allow);
}
echo stripWith(['key' => 'p']);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen_support::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("__rt_array_to_mixed"), "{target}: normalize packed scalar slots");
    assert!(assembly.contains("__rt_decref_array"), "{target}: retire the normalized owner");
    assert!(assembly.contains("__rt_strip_tags"), "{target}");
}

/// Schedules each supported target independently under the CI timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Checks boxed allow-list normalization for this supported target.
        #[test]
        fn $name() { verify($target); }
    };
}

target_test!(strip_tags_review_macos, "macos-aarch64");
target_test!(strip_tags_review_ios, "ios-arm64");
target_test!(strip_tags_review_ios_sim, "ios-sim-arm64");
target_test!(strip_tags_review_linux_arm, "linux-aarch64");
target_test!(strip_tags_review_linux_x86, "linux-x86_64");
