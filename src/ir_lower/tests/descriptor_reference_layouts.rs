//! Purpose:
//! Pins the storage shapes of signature-unknown callable reference arguments.
//!
//! Called from:
//! - AST-to-EIR regression CI for all supported targets.
//!
//! Key details:
//! - Fresh local arguments widen before marker construction; existing shared aliases keep their ABI.

use crate::{codegen::platform::Target, ir::Op, types::PhpType};
use std::path::Path;

/// Packed literal writes consume a fresh object's owner only after rooting the refusal Error.
#[test]
fn packed_nullable_refusal_protects_the_transferred_object_on_every_target() {
    let source = r#"<?php
class PackedNullableRefusalReplacement {
    public function __destruct() { throw new Exception('cleanup'); }
}
function packedNullableRefusalIr(&...$items): void {
    try { $items[0] = new PackedNullableRefusalReplacement(); }
    catch (Throwable $error) { echo $error->getMessage(); }
}
$value = null; packedNullableRefusalIr($value);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, Path::new("main.php"),
            Path::new("."), Target::parse(name).unwrap());
        let function = module.functions.iter().find(|function| function.name == "packedNullableRefusalIr").unwrap();
        let write = function.instructions.iter().find(|inst| inst.op == Op::ArraySet).unwrap();
        let value = write.operands[2];
        let replacement = function.value(value).unwrap();
        assert_eq!(replacement.php_type, PhpType::Object("PackedNullableRefusalReplacement".into()), "{name}");
        assert_eq!(replacement.ownership, crate::ir::Ownership::Owned, "{name}");
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release
            && inst.operands.first() == Some(&value)), "{name}: the setter must consume the owner");
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::StoreLocal
            && inst.operands == [value]), "{name}: a scoped owner would hide premature setter cleanup");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let refusal = &assembly[assembly.find("__rt_tagged_reference_assignment_error_new").unwrap()..];
        let cleanup = refusal.find("__rt_cleanup_preserve_exception").unwrap();
        let thrown = refusal.find("__rt_throw_current").unwrap();
        assert!(cleanup < thrown, "{name}: {refusal}");
    }
}

/// A copied named variadic reaches marker validation before any consuming hash conversion.
#[test]
fn nullable_variadic_copies_use_mixed_key_string_writes_on_every_target() {
    let source = r#"<?php
function nullableCopiedWriteIr(&...$items): void {
    $copy = $items;
    try { $copy['last'] = 'invalid'; } catch (Error $error) { echo count($copy); }
    $copy['extra'] = 7;
}
$value = null; nullableCopiedWriteIr(last: $value);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, Path::new("main.php"),
            Path::new("."), Target::parse(name).unwrap());
        let function = module.functions.iter().find(|function| function.name == "nullableCopiedWriteIr").unwrap();
        let writes = function.instructions.iter().filter(|inst| inst.op == Op::ArraySetMixedKey).collect::<Vec<_>>();
        assert_eq!(writes.len(), 2, "{name}");
        for write in writes {
            let crate::ir::ValueDef::Instruction { inst, .. } = function.value(write.operands[0]).unwrap().def
                else { panic!("{name}: copied destination must be a local load"); };
            assert_eq!(function.instruction(inst).unwrap().op, Op::LoadLocal, "{name}");
        }
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::ArrayToHash),
            "{name}: consuming conversion must not precede a nullable marker refusal");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A native generic by-reference parameter reaches the retiring mixed-key storeback entry.
#[test]
fn nullable_mixed_key_ref_cell_destinations_select_owned_refusal_entries_on_every_target() {
    let source = r#"<?php
function nullableRetiringWriteIr(array<mixed> &$items): void {
    $key = array_key_first($items);
    try { $items[$key] = 'invalid'; } catch (Error $error) { echo count($items); }
}
function nullableRetiringWrapperIr(&...$items): void { nullableRetiringWriteIr($items); }
$value = null; nullableRetiringWrapperIr($value);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, Path::new("main.php"),
            Path::new("."), Target::parse(name).unwrap());
        let function = module.functions.iter().find(|function| function.name == "nullableRetiringWriteIr").unwrap();
        let write = function.instructions.iter().find(|inst| inst.op == Op::ArraySetMixedKey).unwrap();
        let crate::ir::ValueDef::Instruction { inst, .. } = function.value(write.operands[0]).unwrap().def
            else { panic!("retiring destination needs a local reference load"); };
        assert_eq!(function.instruction(inst).unwrap().op, Op::LoadRefCell, "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_array_set_mixed_key_owned_refusal"), "{name}");
    }
}

/// Runtime Iterable dispatch distinguishes boxed fresh arguments from existing raw aliases.
#[test]
fn unknown_descriptor_iterable_reference_layouts_remain_explicit_on_every_target() {
    let source = r#"<?php
class ExplicitIterableMethods {
    public function left(iterable &$items): void {
        $count = 0; foreach ($items as $unused) { $count++; } echo $count;
    }
    public function right(iterable &$items): void {
        $count = 0; foreach ($items as $unused) { $count++; } echo $count;
    }
}
class ExplicitIterableHolder { public iterable $items = [1, 2]; }
function explicitIterableCaller(bool $left): void {
    $writer = new ExplicitIterableMethods(); $method = $left ? 'left' : 'right';
    $array = [1, 2]; $object = new ArrayObject([1, 2]);
    try { $writer->$method($array); } catch (Error $error) { echo count($array); }
    try { $writer->$method($object); } catch (Error $error) { echo count($object); }
    $holder = new ExplicitIterableHolder(); $alias =& $holder->items;
    $writer->$method($alias);
}
explicitIterableCaller($argc > 0);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let caller = module.functions.iter().find(|function| function.name == "explicitIterableCaller").unwrap();
        for local_name in ["array", "object"] {
            let local = caller.locals.iter().find(|local| local.name.as_deref() == Some(local_name)).unwrap();
            assert_eq!(local.php_type.codegen_repr(), PhpType::Mixed, "{name}: {local_name} stays boxed");
        }
        let alias = caller.locals.iter().find(|local| local.name.as_deref() == Some("alias")).unwrap();
        assert_eq!(alias.php_type.codegen_repr(), PhpType::Iterable, "{name}: an existing alias keeps its raw slot");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_tagged_reference_argument_error") && assembly.contains("__rt_heap_kind"),
            "{name}: the runtime-selected parameter must check the physical layout");
    }
}

/// Runtime union keys keep the marker-write regression on ArraySetMixedKey rather than literal paths.
#[test]
fn nullable_reference_marker_runtime_keys_lower_to_mixed_key_writes_on_every_target() {
    let source = r#"<?php
function nullableMixedKeyTarget(&...$items): void {
    $key = array_key_first($items);
    try { $items[$key] = 'invalid'; } catch (Error $error) { echo count($items); }
    $key = json_decode('"0"');
    try { $items[$key] = 'invalid string key'; } catch (Error $error) { echo count($items); }
    $items[$key] = 7; $items[$key] = null;
}
$value = null; nullableMixedKeyTarget($value);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "nullableMixedKeyTarget").unwrap();
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::ArraySetMixedKey).count(), 4,
            "{name}: a literal-key regression would miss the consuming promotion path");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Positional and named arguments use boxed fresh locals without rebinding nullable aliases.
#[test]
fn unknown_descriptor_reference_markers_keep_their_actual_layout_on_every_target() {
    let source = r#"<?php
class UnknownDescriptorProperty { public ?int $value = null; }
function unknownDescriptorLeft(&...$items): void { $items[0] = 'changed'; }
function unknownDescriptorRight(&...$items): void { $items['last'] = 'changed'; }
class UnknownDescriptorMethods {
    public function left(&$first, &$second): void { $first = 'method'; $second = 'reference'; }
    public function right(&$first, &$second): void { $first = 'method'; $second = 'reference'; }
}
function unknownDescriptorCaller(bool $left): void {
    $target = $left ? 'unknownDescriptorLeft' : 'unknownDescriptorRight';
    $first = 1; $second = null;
    $target($first, last: $second);
    $writer = new UnknownDescriptorMethods(); $method = $left ? 'left' : 'right';
    $third = 3; $fourth = null; $writer->$method($third, $fourth);
    $holder = new UnknownDescriptorProperty(); $alias =& $holder->value;
    $target($alias, last: $alias);
}
unknownDescriptorCaller($argc > 0);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let caller = module.functions.iter().find(|function| function.name == "unknownDescriptorCaller").unwrap();
        for local_name in ["first", "second", "third", "fourth"] {
            let local = caller.locals.iter().find(|local| local.name.as_deref() == Some(local_name)).unwrap();
            assert_eq!(local.php_type.codegen_repr(), PhpType::Mixed, "{name}: {local_name}");
            assert!(local.ref_cell_owner.is_some(), "{name}: {local_name} needs a durable boxed cell");
        }
        let alias = caller.locals.iter().find(|local| local.name.as_deref() == Some("alias")).unwrap();
        assert_eq!(alias.php_type.codegen_repr(), PhpType::TaggedScalar, "{name}: shared alias was reshaped");
        assert_eq!(caller.instructions.iter().filter(|inst| inst.op == Op::InvokerRefArg).count(), 6,
            "{name}: named arguments must carry reference markers too");
        for function_name in ["unknownDescriptorLeft", "unknownDescriptorRight"] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            let write = function.instructions.iter().find(|inst| matches!(inst.op,
                Op::ArraySet | Op::HashSet | Op::ArraySetMixedKey)).unwrap();
            assert!(write.effects.contains(crate::ir::Effects::MAY_THROW | crate::ir::Effects::ALLOC_HEAP),
                "{name}: {function_name} must preserve the catchable marker-write boundary");
        }
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_tagged_reference_assignment_error"), "{name}: missing nullable write guard");
    }
}
