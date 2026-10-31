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

/// Runtime union keys keep the marker-write regression on ArraySetMixedKey rather than literal paths.
#[test]
fn nullable_reference_marker_runtime_keys_lower_to_mixed_key_writes_on_every_target() {
    let source = r#"<?php
function nullableMixedKeyTarget(&...$items): void {
    $key = array_key_first($items);
    try { $items[$key] = 'invalid'; } catch (Error $error) { echo count($items); }
    $items[$key] = 7; $items[$key] = null;
}
$value = null; nullableMixedKeyTarget($value);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "nullableMixedKeyTarget").unwrap();
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::ArraySetMixedKey).count(), 3,
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
