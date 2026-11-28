//! Purpose:
//! Verifies switch initialization facts before lexical scope construction on every target.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the library test harness.
//!
//! Key details:
//! - Runtime scope enumeration excludes a later label until that label executes.
//! - Single and joined dispatch edges must not inherit a later label's assignment.

use std::path::Path;

use crate::codegen::platform::Target;


/// Both dispatch shapes build the earlier scope without the later label's local on all targets.
#[test]
fn switch_initialization_scope_inventory_on_all_targets() {
    let source = r#"<?php
function single_scope(int $n): void {
    switch (true) {
        case $n === 1: echo count(get_defined_vars()), "early"; break;
        case ($x = $n + 6) > 0 && $n === 2: echo count(get_defined_vars()), "selected"; break;
        default: echo count(get_defined_vars()), "default";
    }
}
function joined_scope(int $n): void {
    switch ($n) {
        case 1:
        case 2: echo count(get_defined_vars()), "early"; break;
        case ($x = 7) - 4: echo count(get_defined_vars()), "selected"; break;
        default: echo count(get_defined_vars()), "default";
    }
}
single_scope($argc);
single_scope($argc + 1);
single_scope($argc - 1);
joined_scope($argc);
joined_scope($argc + 1);
joined_scope($argc - 1);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(target).expect("supported target"),
        );
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        if module.target == Target::detect_host() {
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout),
                "1early2selected2default1early1early2default", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
