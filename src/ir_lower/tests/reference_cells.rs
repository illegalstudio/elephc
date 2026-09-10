//! Purpose:
//! Checks explicit heap reference-cell ownership in lowered EIR and target assembly.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Heap cell owners are distinct from dereferenced PHP values and borrowed element addresses.
//! - Every supported target must emit balanced retain and retirement helpers.

use crate::codegen::platform::Target;
use crate::ir::{Effects, Immediate, LocalKind, Op};
use std::path::Path;

/// Named regular references use caller element addresses for direct, method and spread calls.
#[test]
fn named_array_element_references_preserve_places_on_every_target() {
    let source = r#"<?php
function namedReference(int $prefix, mixed &$value): void { $value = "changed"; }
class NamedReferenceWriter {
    public function write(int $prefix, mixed &$value): void { $value = "method"; }
    public static function writeStatic(int $prefix, mixed &$value): void { $value = "static"; }
}
$direct = [1]; namedReference(value: $direct[0], prefix: 0);
$writer = new NamedReferenceWriter();
$method = [2]; $writer->write(value: $method[0], prefix: 0);
$static = [3]; NamedReferenceWriter::writeStatic(value: $static[0], prefix: 0);
$spread = [4]; namedReference(...[0], value: $spread[0]);
echo $direct[0], $method[0], $static[0], $spread[0];
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let addresses = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|inst| inst.op == Op::ArrayElemAddr).collect::<Vec<_>>();
        assert_eq!(addresses.len(), 4, "{name}: each named place needs its actual element address");
        for address in addresses {
            assert_eq!(address.result_php_type, crate::types::PhpType::Pointer(None), "{name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Captured locals receive explicit cell owners before any descriptor can borrow their addresses.
#[test]
fn closure_local_reference_cells_have_frame_owners_on_every_target() {
    let source = r#"<?php
function localReferenceClosure(string $text): callable {
    $counter = 0;
    return function() use (&$text, &$counter): string { $counter++; return $text; };
}
$callback = localReferenceClosure("owned");
echo $callback();
unset($callback);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "localReferenceClosure").unwrap();
        let promotions = function.instructions.iter().enumerate().filter(|(_, inst)| inst.op == Op::PromoteLocalRefCell)
            .collect::<Vec<_>>();
        assert_eq!(promotions.len(), 2, "{name}");
        let closure = function.instructions.iter().position(|inst| inst.op == Op::ClosureNew).unwrap();
        for (index, inst) in promotions {
            let Some(Immediate::LocalSlotPair { second: owner, .. }) = inst.immediate else { panic!("{name}: missing owner"); };
            assert_eq!(function.locals[owner.as_raw() as usize].kind, LocalKind::RefCell, "{name}");
            assert!(index < closure, "{name}: promote before capturing the cell address");
        }
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_reference_cell_new"), "{name}");
        assert!(asm.contains("__rt_local_ref_cell_release"), "{name}");
    }
}

/// Closure construction retains any managed cell behind a captured native reference argument.
#[test]
fn closures_retain_managed_reference_arguments_on_every_target() {
    let source = r#"<?php
function captureManagedArgument(array &$items): callable {
    return function() use (&$items): int { return count($items); };
}
$items = [1];
$callback = captureManagedArgument($items);
echo $callback();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let asm = asm.split_once("captureManagedArgument:\n").unwrap().1;
        let owner = asm.find("__rt_reference_cell_owner").expect("capture checks for an owned cell");
        let next_call = if name == "linux-x86_64" {
            asm[owner..].lines().skip(1).find(|line| line.trim_start().starts_with("call "))
        } else {
            asm[owner..].lines().skip(1).find(|line| line.trim_start().starts_with("bl "))
        }.expect("managed capture retention");
        assert!(next_call.contains("__rt_incref"), "{name}: {next_call}");
    }
}

/// A binding first encountered inside a loop retires the owner retained by earlier iterations.
#[test]
fn repeated_reference_aliases_retire_the_previous_owner_on_every_target() {
    let source = r#"<?php
function repeatReferenceAlias(): void {
    $value = 0;
    for ($i = 0; $i < 5; $i++) { $alias = &$value; $alias = $i; }
    echo $value, '|', $alias;
}
repeatReferenceAlias();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("repeatReferenceAlias")).unwrap();
        let (retained, owner) = function.instructions.iter().enumerate().find_map(|(index, inst)| {
            if inst.op != Op::RetainLocalRefCell { return None; }
            let Some(Immediate::LocalSlotPair { second, .. }) = inst.immediate else { return None; };
            Some((index, second))
        }).expect("the alias retains its own cell owner");
        assert!(function.instructions[..retained].iter().any(|inst| {
            inst.op == Op::ReleaseLocalRefCell && inst.immediate == Some(Immediate::LocalSlot(owner))
        }), "{name}: loop aliases retire their previous owner before retaining another");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Rebinding a local to its own property retains the cell before retiring the old object slot.
#[test]
fn reference_rebinding_retires_the_previous_slot_owner_on_every_target() {
    let source = r#"<?php
class ReboundReferenceOwner { public array $items = [6]; }
function rebindReferenceOwner(): void {
    $holder = new ReboundReferenceOwner();
    $holder = &$holder->items;
    echo $holder[0];
    unset($holder);
}
rebindReferenceOwner();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("rebindReferenceOwner")).unwrap();
        let holder = function.locals.iter().find(|local| local.name.as_deref() == Some("holder")).unwrap().id;
        let retained = function.instructions.iter().position(|inst| inst.op == Op::BindRefCellPtr
            && matches!(inst.immediate, Some(Immediate::LocalSlotPair { .. }))).unwrap();
        let released = function.instructions.iter().enumerate().skip(retained + 1)
            .find(|(_, inst)| inst.op == Op::ReleaseLocalSlot
                && inst.immediate == Some(Immediate::LocalSlot(holder)))
            .map(|(index, _)| index).expect("old object slot is retired explicitly");
        let rebound = function.instructions.iter().position(|inst| inst.op == Op::AliasLocalRefCell
            && matches!(inst.immediate, Some(Immediate::LocalSlotPair { first, .. }) if first == holder)).unwrap();
        assert!(retained < released && released < rebound,
            "{name}: retain the new cell, retire the old value, then publish the alias");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Resolved returns lease their managed cells until the caller adopts or copies their values.
#[test]
fn reference_returns_transfer_cell_owners_on_every_target() {
    let source = r#"<?php
class ReturningReferenceOwner {
    public string $text = 'value';
    public function &reference(): string { return $this->text; }
}
function &createReturnedReference(): string {
    $object = new ReturningReferenceOwner();
    return $object->text;
}
function consumeReturnedReference(): void {
    $alias = &createReturnedReference();
    $copy = createReturnedReference();
    $method = &(new ReturningReferenceOwner())->reference();
    echo $alias, $copy, $method;
}
consumeReturnedReference();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let callee = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("createReturnedReference")).unwrap();
        let acquisition = callee.instructions.iter().find(|inst| inst.op == Op::AcquireRefCell)
            .expect("callee retains a returned property cell before destroying its local object");
        let Some(Immediate::LocalSlot(owner)) = acquisition.immediate else { unreachable!(); };
        assert_eq!(callee.locals[owner.as_raw() as usize].kind, LocalKind::ReturnRefCell, "{name}");
        assert!(acquisition.effects.contains(Effects::REFCOUNT_OP | Effects::WRITES_LOCAL), "{name}");
        assert!(acquisition.effects.contains(Effects::MAY_THROW),
            "{name}: replacing a pending reference return can run a payload destructor");
        let caller = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("consumeReturnedReference")).unwrap();
        assert!(caller.instructions.iter().filter(|inst| inst.op == Op::AdoptRefCellPtr).count() >= 3,
            "{name}: aliases and value copies consume the returned lease before argument cleanup");
        assert!(caller.instructions.iter().any(|inst| inst.op == Op::LoadRefCell),
            "{name}: ordinary calls read the referenced value, not the cell address");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_reference_cell_owner"), "{name}");
        assert!(assembly.contains("__rt_local_ref_cell_release"), "{name}");
    }
}

/// Local aliases retain cells, and known object-owned property aliases carry dedicated owner slots.
#[test]
fn reference_alias_owners_are_explicit_on_every_target() {
    let source = r#"<?php
class ReferenceOwner { public array $items = [1]; }
function createReferenceOwners(): void {
    $original = [1, 2];
    $first = &$original;
    $last = &$first;
    unset($original, $first);
    echo $last[0];
    $object = new ReferenceOwner();
    $property = &$object->items;
    $copy = clone $object;
    echo $property[0];
}
createReferenceOwners();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("createReferenceOwners")).unwrap();
        let retained = function.instructions.iter().filter(|inst| inst.op == Op::RetainLocalRefCell).count();
        assert!(retained >= 2, "{name}: both local aliases retain their cell owner");
        let binding = function.instructions.iter().find(|inst| {
            inst.op == Op::BindRefCellPtr && matches!(inst.immediate, Some(Immediate::LocalSlotPair { .. }))
        }).expect("property alias carries an owned heap cell");
        let Some(Immediate::LocalSlotPair { second: owner, .. }) = binding.immediate else { unreachable!(); };
        assert_eq!(function.locals[owner.as_raw() as usize].kind, LocalKind::RefCell);
        assert_eq!(function.value(binding.operands[0]).unwrap().php_type, crate::types::PhpType::Pointer(None));
        assert!(binding.effects.contains(Effects::REFCOUNT_OP | Effects::WRITES_HEAP));
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_incref"), "{name}");
        assert!(assembly.contains("__rt_local_ref_cell_release"), "{name}");
        assert!(assembly.contains("__rt_reference_cell_new"), "{name}: property cells have typed headers");
        assert!(assembly.contains("__rt_reference_cell_clone"), "{name}: clone preserves reference ownership");
    }
}
