//! Purpose:
//! Verifies conditional joins retain only path-independent Reflection targets.
//!
//! Called from:
//! - AST-to-EIR unit coverage for all supported targets.
//!
//! Key details:
//! - Static-property writes must reach live storage after an unrelated conditional.
//! - Sibling rebindings must not contaminate the other branch or its merge.

use crate::ir::{Immediate, Op};

/// Keeps unchanged and equal targets while rejecting conflicting reflected classes.
#[test]
fn reflection_static_writes_preserve_conditional_targets() {
    let source = r#"<?php
class BranchReflectedA { public static string $value = ''; }
class BranchReflectedB { public static string $value = ''; }
function unchangedReflectionTarget(bool $flag): void {
    $class = new ReflectionClass(BranchReflectedA::class);
    if ($flag) { echo 'side-effect'; }
    $class->setStaticPropertyValue('value', 'unchanged');
}
function equalReflectionTargets(bool $flag): void {
    if ($flag) { $class = new ReflectionClass(BranchReflectedB::class); }
    else { $class = new ReflectionClass(BranchReflectedB::class); }
    $class->setStaticPropertyValue('value', 'equal');
}
function conflictingReflectionTargets(bool $flag): void {
    $class = new ReflectionClass(BranchReflectedA::class);
    if ($flag) { $class = new ReflectionClass(BranchReflectedB::class); }
    $class->setStaticPropertyValue('value', 'runtime');
}
function siblingReflectionTargets(bool $flag): void {
    $class = new ReflectionClass(BranchReflectedA::class);
    if ($flag) { $class = new ReflectionClass(BranchReflectedB::class); }
    else { $class->setStaticPropertyValue('value', 'sibling'); }
}
unchangedReflectionTarget($argc > 1);
equalReflectionTargets($argc > 1);
conflictingReflectionTargets($argc > 1);
siblingReflectionTargets($argc > 1);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for (name, expected) in [
            ("unchangedReflectionTarget", vec!["BranchReflectedA::value"]),
            ("equalReflectionTargets", vec!["BranchReflectedB::value"]),
            ("conflictingReflectionTargets", vec![]),
            ("siblingReflectionTargets", vec!["BranchReflectedA::value"]),
        ] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let writes: std::collections::BTreeSet<_> = function.instructions.iter().filter(|inst|
                inst.op == Op::StoreReflectionStaticProperty
            ).map(|inst| {
                let Some(Immediate::Data(data)) = inst.immediate else { panic!("missing static target"); };
                module.data.strings[data.as_raw() as usize].as_str()
            }).collect();
            assert_eq!(writes, expected.into_iter().collect(), "{target}: {name}");
        }
    }
}
