//! Purpose:
//! Checks that typed reference returns keep stable cell payloads across frame widening.
//!
//! Called from:
//! - The AST-to-EIR regression suite on every supported target.
//!
//! Key details:
//! - Each return creates its own typed cell independently of its source's final slot shape.
//! - Handler recovery is a separate boundary because a throwing finally may expose the cell.

/// Independent switch paths each promote the nullable value instead of trusting a sibling's alias.
#[test]
fn nullable_object_reference_return_switch_siblings_promote_on_every_target() {
    let source = r#"<?php
class SiblingReferenceNode {}
class SiblingReferenceHolder {
    public ?SiblingReferenceNode $node = null;
    public ?SiblingReferenceNode $fallback = null;
}

function &siblingReferenceReturn(SiblingReferenceHolder $holder, int $choice): ?SiblingReferenceNode {
    $node = $holder->node;
    switch ($choice) {
        case 1: return $node;
        default: $node = null; var_dump($node); $node = $holder->fallback; return $node;
    }
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "siblingReferenceReturn").unwrap();
        let aliases = function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::AliasLocalRefCell)
            .filter(|inst| matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlotPair { first, .. })
                if function.locals[first.as_raw() as usize].name.as_deref() == Some("node"))).count();
        assert_eq!(aliases, 2, "{target}: each raw sibling needs its own checked typed cell");
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::AcquireRefCell).count(), 2,
            "{target}: both return paths transfer their own cell lease");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Earlier and later nullable stores cannot make a typed return transfer an in-place Mixed cell.
#[test]
fn concrete_reference_returns_stage_before_and_after_frame_widening_on_every_target() {
    let source = r#"<?php
class WidenedReferenceNode {}
class WidenedReferenceHolder {
    public ?WidenedReferenceNode $node = null;
    public ?string $text = null;
}
function &widenedReferenceBefore(WidenedReferenceHolder $holder): WidenedReferenceNode {
    $value = $holder->node; $value = new WidenedReferenceNode();
    if (!$value instanceof WidenedReferenceNode) { throw new TypeError('node'); }
    return $value;
}
function &widenedReferenceAfter(WidenedReferenceHolder $holder, bool $first): WidenedReferenceNode {
    $value = new WidenedReferenceNode();
    if ($first) { return $value; }
    $value = $holder->node; $value = new WidenedReferenceNode();
    if (!$value instanceof WidenedReferenceNode) { throw new TypeError('node'); }
    return $value;
}
function &widenedReferenceString(WidenedReferenceHolder $holder, bool $first): string {
    $value = 'early'; if ($first) { return $value; }
    $value = $holder->text; $value = 'late';
    if (!is_string($value)) { throw new TypeError('text'); }
    return $value;
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        for (name, returns) in [("widenedReferenceBefore", 1), ("widenedReferenceAfter", 2),
            ("widenedReferenceString", 2)] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let source_slot = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap().id;
            assert!(matches!(function.locals[source_slot.as_raw() as usize].php_type.codegen_repr(),
                crate::types::PhpType::Mixed), "{target}: {name} must exercise final Mixed storage");
            assert!(!function.instructions.iter().any(|inst| inst.op == crate::ir::Op::PromoteLocalRefCell
                && matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlotPair { first, .. })
                    if first == source_slot)), "{target}: {name} cannot transfer the source slot's Mixed payload");
            assert_eq!(function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::AcquireRefCell).count(),
                returns, "{target}: {name} must transfer every typed result");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Typed reference staging gives each consuming container conversion an independent owner.
#[test]
fn reference_return_container_conversions_acquire_their_input_on_every_target() {
    let source = r#"<?php
function &uniqueReferenceContainerIr(): array<mixed> { array<int> $items = [1, 2]; return $items; }
function &sharedReferenceContainerIr(array<int> $input): array<mixed> { array<int> $items = $input; return $items; }
function &emptyReferenceHashIr(): array<string, int> { array<int> $items = []; return $items; }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        for (name, conversion) in [("uniqueReferenceContainerIr", crate::ir::Op::ArrayToMixed),
            ("sharedReferenceContainerIr", crate::ir::Op::ArrayToMixed),
            ("emptyReferenceHashIr", crate::ir::Op::ArrayToHash)] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let converted = function.instructions.iter().find(|inst| inst.op == conversion).unwrap();
            let value = function.value(converted.operands[0]).unwrap();
            let crate::ir::ValueDef::Instruction { inst, .. } = value.def else { panic!("conversion needs an owned producer"); };
            assert_eq!(function.instruction(inst).unwrap().op, crate::ir::Op::Acquire,
                "{target}: {name} must not consume the source slot's reference");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Duplicated and nested reference returns name independent leases for every captured snapshot.
#[test]
fn pending_reference_return_sites_have_distinct_lease_slots_on_every_target() {
    let source = r#"<?php
class NestedPendingReferenceNode {}
function &nestedPendingReference(): NestedPendingReferenceNode {
    $outer = new NestedPendingReferenceNode(); $inner = new NestedPendingReferenceNode();
    try { return $outer; }
    finally {
        try { try { return $inner; } finally { throw new Exception('cancel inner'); } }
        catch (Exception $error) { echo 'caught'; }
    }
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "nestedPendingReference").unwrap();
        let acquisitions = function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::AcquireRefCell)
            .collect::<Vec<_>>();
        let owners = acquisitions.iter().map(|inst| {
            let Some(crate::ir::Immediate::LocalSlot(owner)) = inst.immediate else { panic!("missing return lease"); };
            assert_eq!(function.locals[owner.as_raw() as usize].kind, crate::ir::LocalKind::ReturnRefCell);
            owner
        }).collect::<std::collections::HashSet<_>>();
        assert!(owners.len() >= 2, "{target}: exercise independent inner and outer returns");
        assert_eq!(owners.len(), acquisitions.len(), "{target}: each emitted return site needs its own owner");
        for owner in &owners {
            assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::PushCallOperandOwner
                && inst.immediate == Some(crate::ir::Immediate::LocalSlot(*owner))),
                "{target}: each pending lease must retire before a same-frame catch");
        }
        for block in &function.blocks {
            if let Some(crate::ir::Terminator::Return { value: Some(value) }) = block.terminator {
                let crate::ir::ValueDef::Instruction { inst, .. } = function.value(value).unwrap().def else {
                    panic!("return must transport an acquisition snapshot");
                };
                assert_eq!(function.instruction(inst).unwrap().op, crate::ir::Op::AcquireRefCell, "{target}");
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
