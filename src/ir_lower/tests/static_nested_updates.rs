//! Purpose:
//! Checks static nested array update parents and unwind-visible owners on every target.
//!
//! Called from:
//! - The AST-to-EIR unit suite through `crate::ir_lower::tests`.
//!
//! Key details:
//! - Parent reads diagnose keys before write-context autovivification.
//! - A pending parent remains rooted across a warning handler or throwing RHS.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, LocalKind, Op, RuntimeCallTarget};
use crate::types::PhpType;
use std::path::Path;

/// Verifies write-context parent lookup, literal-key ownership, and target-aware emission.
fn check_static_nested_updates(target: &str) {
    let source = r#"<?php
class NestedUpdate { public static array $items = [[null]]; }
class ConcreteUpdate { public static $items = [1, [5]]; }
function fail(): int { throw new Error("stop"); }
function finalKey(): int { echo "key"; return 0; }
function scalarUpdate(): void { ++NestedUpdate::$items[0][0][finalKey()]; }
set_error_handler(function($level, $message) { return true; });
++NestedUpdate::$items[0][0]["before"];
++ConcreteUpdate::$items[1][0];
try { scalarUpdate(); } catch (Error $e) { echo "scalar"; }
try { NestedUpdate::$items[0][0]["before"] += fail(); } catch (Error $e) { echo "caught"; }
restore_error_handler();
echo json_encode(NestedUpdate::$items);
"#;
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
    );
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    assert!(main.instructions.iter().any(|inst| {
        inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed))
    }), "{target}: intermediate parents use write-context lookup");
    assert!(main.instructions.iter().any(|inst| {
        inst.op == Op::PushCallOperandOwner && matches!(inst.immediate,
            Some(Immediate::LocalSlot(slot))
                if main.locals[slot.as_raw() as usize].php_type.codegen_repr() == PhpType::Mixed)
    }), "{target}: pending parent cells are visible to the unwinder");
    assert!(!main.locals.iter().any(|local| {
        local.kind == LocalKind::PhpLocal && local.php_type == PhpType::Str
    }), "{target}: immutable literal keys do not become string-owning capture locals");
    assert!(main.instructions.iter().any(|inst| inst.op == Op::StrIncDec),
        "{target}: static incdec uses the PHP string/null kernel");
    let scalar = module.functions.iter().find(|function| function.name == "scalarUpdate").unwrap();
    assert!(scalar.blocks.iter().any(|block| block.name.starts_with("static.update.scalar")
        && matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))),
        "{target}: a scalar parent throws through ordinary EIR exception control flow");
    let key = scalar.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
    let guard = scalar.instructions.iter().rposition(|inst| inst.op == Op::TypePredicate).unwrap();
    assert!(key < guard, "{target}: final computed key runs before the scalar-parent guard");
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Gives each target its own CI timeout while retaining the complete supported matrix.
macro_rules! static_nested_update_target_test {
    ($name:ident, $target:literal) => {
        /// Checks static nested updates and parent-owner emission on one supported target.
        #[test]
        fn $name() { check_static_nested_updates($target); }
    };
}

static_nested_update_target_test!(static_nested_update_macos, "macos-aarch64");
static_nested_update_target_test!(static_nested_update_ios_device, "ios-arm64");
static_nested_update_target_test!(static_nested_update_ios_simulator, "ios-sim-arm64");
static_nested_update_target_test!(static_nested_update_linux_arm64, "linux-aarch64");
static_nested_update_target_test!(static_nested_update_linux_x86_64, "linux-x86_64");
