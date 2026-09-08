//! Purpose:
//! Verifies inherited builtin Throwable constructors have callable, layout-aware EIR bodies.
//!
//! Called from:
//! - The AST-to-EIR regression tests on every supported target.
//!
//! Key details:
//! - Ordinary subclasses add storage while Error and Exception retain their shared method owners.

use super::*;
use crate::ir::{Effects, Immediate, RuntimeCallTarget};

/// Both constructor roots remain emitted and use normalized boxed previous operands on all targets.
#[test]
fn inherited_throwable_constructors_have_layout_aware_bodies() {
    let source = r#"<?php
class ExtendedException extends Exception { public int $marker = 42; }
class ExtendedError extends Error { public string $marker = "kept"; }
$error = new ExtendedException("outer", 1, new ExtendedError("inner"));
echo $error->getMessage();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap());
        for owner in ["Error", "Exception"] {
            let constructor = module.class_methods.iter()
                .find(|function| function.name == format!("{owner}::__construct"))
                .unwrap_or_else(|| panic!("{name}: missing {owner} constructor"));
            let initialize = constructor.instructions.iter().find(|inst| {
                inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ThrowableInitialize))
            }).expect("constructor must initialize the receiver's concrete layout");
            assert_eq!(initialize.operands.len(), 4, "{name}");
            assert!(initialize.effects.contains(Effects::MAY_THROW | Effects::REFCOUNT_OP), "{name}");
        }
    }
}
