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

/// Keeps common targets and retains both guarded stores when reflected classes conflict.
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

/// A reflected target is rooted before the RHS call and retired after its value is stored.
fn verify_reflection_target_leases(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class ReflectionTargetLease { public string $value = ''; }
function reflectionLeaseValue(): string { return 'value'; }
function assignReflectionLease(): void {
    $property = new ReflectionProperty(ReflectionTargetLease::class, 'value');
    $property->setValue(new ReflectionTargetLease(), reflectionLeaseValue());
}
function initializedReflectionLease(): bool {
    $property = new ReflectionProperty(ReflectionTargetLease::class, 'value');
    return $property->isInitialized(new ReflectionTargetLease());
}
assignReflectionLease(); echo initializedReflectionLease();
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let setter = module.functions.iter().find(|function| function.name == "assignReflectionLease").unwrap();
    let store = setter.instructions.iter().position(|inst| inst.op == Op::PropSet
        && matches!(inst.immediate, Some(Immediate::ReflectionPropertyRef { .. }))).unwrap();
    let object = setter.instructions[store].operands[0];
    let slot = setter.instructions.iter().find_map(|inst| match inst.immediate {
        Some(Immediate::LocalSlot(slot)) if inst.op == Op::StoreLocal && inst.operands == [object] => Some(slot),
        _ => None,
    }).expect("the reflected target must have an independent cleanup root");
    let publish = setter.instructions.iter().position(|inst| inst.op == Op::PushCallOperandOwner
        && inst.immediate == Some(Immediate::LocalSlot(slot))).unwrap();
    let rhs = setter.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
    let retire = setter.instructions.iter().position(|inst| inst.op == Op::ReleaseLocalSlot
        && inst.immediate == Some(Immediate::LocalSlot(slot))).unwrap();
    assert!(publish < rhs && rhs < store && store < retire,
        "{target}: the receiver root must cover RHS evaluation, coercion and the store");
    let initialized = module.functions.iter().find(|function| function.name == "initializedReflectionLease").unwrap();
    let probe = initialized.instructions.iter().position(|inst| inst.op == Op::PropInitialized).unwrap();
    let object = initialized.instructions[probe].operands[0];
    assert!(initialized.instructions[probe + 1..].iter().any(|inst|
        inst.op == Op::Release && inst.operands == [object]),
        "{target}: an initialized probe must retire its owning argument");
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(reflection_target_leases, verify_reflection_target_leases);
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
            ("conflictingReflectionTargets", vec!["BranchReflectedA::value", "BranchReflectedB::value"]),
            ("siblingReflectionTargets", vec!["BranchReflectedA::value"]),
        ] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let writes: std::collections::BTreeSet<_> = function.instructions.iter().filter(|inst|
                inst.op == Op::StoreReflectionStaticProperty
            ).map(|inst| {
                let Some(Immediate::Data(data)) = inst.immediate else { panic!("missing static target"); };
                module.data.strings[data.as_raw() as usize].as_str()
            }).collect();
            assert_eq!(writes, expected.into_iter().collect::<std::collections::BTreeSet<_>>(), "{target}: {name}");
        }
    }
}
