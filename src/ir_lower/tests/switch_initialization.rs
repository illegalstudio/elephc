//! Purpose:
//! Verifies switch initialization facts before lexical scope construction on every target.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the library test harness.
//!
//! Key details:
//! - Scope-array capacities expose definitely visible PHP locals at each case body.
//! - Single and joined dispatch edges must not inherit a later label's assignment.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op};

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
joined_scope($argc);
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
        for name in ["single_scope", "joined_scope"] {
            let function = module.functions.iter()
                .find(|function| function.name == name)
                .expect("scope fixture survives optimization");
            let capacities = function.instructions.iter()
                .filter_map(|instruction| {
                    if instruction.op != Op::HashNew {
                        return None;
                    }
                    match instruction.immediate {
                        Some(Immediate::Capacity(capacity)) => Some(capacity),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(capacities, [1, 2, 2], "{target:?}: {name} scope inventory");
        }
    }
}
