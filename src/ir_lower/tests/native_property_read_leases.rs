//! Purpose:
//! Checks native property read captures and nullable probes at their EIR ownership boundary.
//!
//! Called from:
//! - The AST-to-EIR target regression suite.
//!
//! Key details:
//! - A published update snapshot lends its value until its slot retires.
//! - A nullable native coalesce probe must retain nullsafe dispatch.

use crate::ir::{Immediate, LocalKind, Op};

/// Every target preserves the snapshot owner and quiet null receiver behavior.
fn verify_native_property_read_leases(target: &str) {
    let source = r#"<?php
class NativeReadCounter { public mixed $value = 1; }
function nativeReadCompound(NativeReadCounter $box, string $name): void {
    $box->{$name} += 1;
    $copy = ($box->{$name} += 1);
}
function nativeReadNullable(?SimpleXMLElement $node): mixed {
    return $node->missing ?? 'fallback';
}
function nativeReadContainerMiss(int $index): mixed {
    $rows = [new SimpleXMLElement('<r/>')];
    return $rows[$index]->missing ?? 'fallback';
}
$xml = new SimpleXMLElement('<r/>');
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let compound = module.functions.iter().find(|function| function.name == "nativeReadCompound").unwrap();
    let mut captures = 0;
    for load in compound.instructions.iter().filter(|inst| inst.op == Op::LoadLocal) {
        let Some(Immediate::LocalSlot(slot)) = load.immediate else { continue; };
        let value = load.result.unwrap();
        if !compound.instructions.iter().any(|inst| inst.op == Op::MixedClone && inst.operands == [value])
            || !compound.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner
                && inst.immediate == Some(Immediate::LocalSlot(slot)))
            || compound.instructions.iter().any(|inst| matches!(inst.op, Op::ZeroLocalSlot | Op::UnsetLocal)
                && inst.immediate == Some(Immediate::LocalSlot(slot)))
        { continue; }
        captures += 1;
        assert_ne!(compound.locals[slot.as_raw() as usize].kind, LocalKind::OwnedTemp,
            "{target}: a published non-taken capture must lend its reads");
        assert!(!compound.instructions.iter().any(|inst| inst.op == Op::Release && inst.operands == [value]),
            "{target}: copying the read must not release the slot's owner");
        assert!(compound.instructions.iter().any(|inst| inst.op == Op::ReleaseLocalSlot
            && inst.immediate == Some(Immediate::LocalSlot(slot))),
            "{target}: the snapshot owner still retires after the update");
    }
    assert!(captures > 0, "{target}: the fixture must exercise a published Mixed read capture");
    let nullable = module.functions.iter().find(|function| function.name == "nativeReadNullable").unwrap();
    assert!(nullable.instructions.iter().any(|inst| inst.op == Op::NullsafeMethodCall
        && matches!(inst.immediate, Some(Immediate::Data(name))
            if module.data.strings[name.as_raw() as usize] == "__elephc_property_probe")),
        "{target}: null coalesce receivers must skip native method dispatch");
    let missing = module.functions.iter().find(|function| function.name == "nativeReadContainerMiss").unwrap();
    let read = missing.instructions.iter().find(|inst| inst.op == Op::ArrayGetSilent)
        .expect("a quiet probe must not warn about its absent array receiver");
    assert!(!missing.instructions.iter().any(|inst| inst.op == Op::ArrayGet),
        "{target}: the property probe must keep the array read silent");
    assert!(missing.instructions.iter().any(|inst| inst.op == Op::IsNull
        && (inst.operands == [read.result.unwrap()]
            || inst.operands.first().is_some_and(|receiver| missing.instructions.iter().any(|acquire|
                acquire.result == Some(*receiver) && acquire.op == Op::Acquire
                    && acquire.operands == [read.result.unwrap()])))),
        "{target}: the raw missing-container sentinel needs an explicit null branch");
}

supported_target_cases!(native_property_read_leases, verify_native_property_read_leases);
