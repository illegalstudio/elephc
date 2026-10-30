//! Purpose:
//! Verifies declared-slot initialization and nested property RW guards on every target.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Concrete nonnullable slots only probe initialization before a write.
//! - Nested updates select a property in RW mode before their first element read.

use crate::ir::Op;

/// Concrete write fetches probe initialization without acquiring an impossible null payload.
#[test]
fn concrete_property_initialization_guards_on_every_target() {
    let source = r#"<?php
class RequiredFetchSlot { public ArrayObject $items; }
function concreteRequiredWrite(RequiredFetchSlot $box): void { $box->items[0] = 1; }
concreteRequiredWrite(new RequiredFetchSlot());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "concreteRequiredWrite").unwrap();
        let probe = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized)
            .expect("a concrete slot requires a write initialization probe");
        let read = function.instructions.iter().position(|inst| inst.op == Op::PropGet)
            .expect("an initialized ArrayAccess object retains its ordinary read");
        assert!(probe < read, "{target}: initialization must precede the property read");
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::IsNull),
            "{target}: initialized concrete object storage cannot contain null");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// All nested update forms reject null before element reads and retain RW uninitialized errors.
#[test]
fn nested_property_readwrite_guards_on_every_target() {
    let source = r#"<?php
class NestedGuardSlot { public ?ArrayObject $items; }
function nestedGuardCompound(mixed $box): void { $box->items[0][1] += 1; }
function nestedGuardIncrement(mixed $box): void { $box->items[0][1]++; }
function nestedGuardExpression(mixed $box): mixed { return ($box->items[0][1] += 1); }
function nestedGuardPrefix(mixed $box): mixed { return ++$box->items[0][1]; }
$box = new NestedGuardSlot();
nestedGuardCompound($box); nestedGuardIncrement($box);
nestedGuardExpression($box); nestedGuardPrefix($box);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["nestedGuardCompound", "nestedGuardIncrement", "nestedGuardExpression", "nestedGuardPrefix"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            let guard = function.instructions.iter().position(|inst| inst.op == Op::IsNull)
                .unwrap_or_else(|| panic!("{target}: {name} requires a null guard"));
            let read = function.instructions.iter().position(|inst| matches!(inst.op,
                Op::RuntimeCall | Op::MethodCall | Op::ArrayGet | Op::ArrayGetMixedKey | Op::HashGet)
                && inst.result.is_some() && inst.operands.len() >= 2
                && inst.result_php_type.codegen_repr() == crate::types::PhpType::Mixed)
                .unwrap_or_else(|| panic!("{target}: {name} must retain its element read"));
            assert!(guard < read, "{target}: {name} must guard before element reads");
            let write = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized);
            assert!(write.is_none_or(|write| guard < write),
                "{target}: {name} must select RW before a later write fetch");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
