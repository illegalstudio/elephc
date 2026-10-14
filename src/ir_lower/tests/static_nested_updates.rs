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
class DeclaredMixedUpdate { public static array<mixed> $items = [1, [5]]; }
class DeclaredAssocUpdate { public static array<string, mixed> $items = ["k" => [5]]; }
class ObjectParent implements ArrayAccess {
    public ArrayObject $child;
    public function __construct() { $this->child = new ArrayObject([1]); }
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return $this->child; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class ObjectUpdate { public static array $items = []; }
function objectLiteralUpdate(): mixed { return ++ObjectUpdate::$items[0]['child'][0]; }
function objectComputedUpdate(int $parent, string $child, int $leaf): mixed {
    return ++ObjectUpdate::$items[$parent][$child][$leaf];
}
function objectPostfixUpdate(): mixed { return ObjectUpdate::$items[0]['child'][0]++; }
function variableStringKey(string $key): void { ++DeclaredAssocUpdate::$items[$key][0]; }
function promotedStringKey(string $key): void { ++DeclaredMixedUpdate::$items[$key][0]; }
function promotedMixedKey(mixed $key): void { ++DeclaredMixedUpdate::$items[$key][0]; }
function flatStringKey(string $key): void { ++DeclaredMixedUpdate::$items[$key]; }
function flatMixedKey(mixed $key): void { ++DeclaredMixedUpdate::$items[$key]; }
function promotedNullableKey(?int $key): void { ++DeclaredMixedUpdate::$items[$key][0]; }
function flatNullableKey(?int $key): void { ++DeclaredMixedUpdate::$items[$key]; }
function assocFloatKey(float $key): void { ++DeclaredAssocUpdate::$items[$key][0]; }
function fail(): int { throw new Error("stop"); }
function finalKey(): int { echo "key"; return 0; }
function scalarUpdate(): void { ++NestedUpdate::$items[0][0][finalKey()]; }
function stringUpdate(): void { --NestedUpdate::$items[0][0]; }
function stringCompound(): void { NestedUpdate::$items[0][0] += 1; }
function stringIntermediate(): void { ++NestedUpdate::$items[0][0][1]; }
function stringIntermediateCompound(): void { NestedUpdate::$items[0][0][1] += 1; }
function nestedPostfix(): mixed { return NestedUpdate::$items[0][0][1]++; }
function falseParentUpdate(): mixed { return ++NestedUpdate::$items[0][0][1]; }
function falsePlainWrite(mixed $key): void { NestedUpdate::$items[0][0][$key] = 9; }
function falsePlainAppend(): void { NestedUpdate::$items[0][0][] = 9; }
function falseLocalWrite(bool $flag): void { $items = [[$flag]]; $items[0][0][1] = 9; }
set_error_handler(function($level, $message) { return true; });
++NestedUpdate::$items[0][0]["before"];
++ConcreteUpdate::$items[1][0];
++DeclaredMixedUpdate::$items[1][0];
++DeclaredAssocUpdate::$items["k"][0];
DeclaredAssocUpdate::$items["k"][0] = 9;
variableStringKey("k");
promotedStringKey("k");
promotedMixedKey("m");
flatStringKey("s");
flatMixedKey("t");
promotedNullableKey(null);
flatNullableKey(null);
assocFloatKey(1.5);
echo ++NestedUpdate::$items[1.5], NestedUpdate::$items[2.5]++;
echo ++NestedUpdate::$items[0][0]["before"];
try { scalarUpdate(); } catch (Error $e) { echo "scalar"; }
try { NestedUpdate::$items[0][0]["before"] += fail(); } catch (Error $e) { echo "caught"; }
restore_error_handler();
echo json_encode(NestedUpdate::$items);
"#;
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
    );
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    assert!(main.instructions.iter().any(|inst| inst.op == Op::LoadStaticPropertyRefCell),
        "{target}: declared mixed roots publish relocated containers through their native slot");
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
    assert!(main.instructions.iter().filter(|inst| {
        inst.op == Op::RuntimeCall && inst.result.is_none()
            && inst.immediate == Some(Immediate::Bool(true))
    }).count() >= 2, "{target}: captured static expression stores reuse diagnosed keys");
    let scalar = module.functions.iter().find(|function| function.name == "scalarUpdate").unwrap();
    assert!(scalar.blocks.iter().any(|block| block.name.starts_with("static.update.scalar")
        && matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))),
        "{target}: a scalar parent throws through ordinary EIR exception control flow");
    let key = scalar.instructions.iter().position(|inst| inst.op == Op::Call).unwrap();
    let guard = scalar.instructions.iter().rposition(|inst| inst.op == Op::TypePredicate).unwrap();
    assert!(key < guard, "{target}: final computed key runs before the scalar-parent guard");
    for name in ["stringUpdate", "stringCompound", "stringIntermediate", "stringIntermediateCompound"] {
        let string = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(string.blocks.iter().any(|block| block.name.starts_with("static.update.string")
            && matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))),
            "{target}: {name} has a distinct catchable string-parent error");
    }
    let postfix = module.functions.iter().find(|function| function.name == "nestedPostfix").unwrap();
    assert!(postfix.instructions.iter().any(|inst| inst.op == Op::MixedClone),
        "{target}: the postfix result owns an independent old-value box");
    let false_parent = module.functions.iter().find(|function| function.name == "falseParentUpdate").unwrap();
    assert!(false_parent.blocks.iter().any(|block| block.name.starts_with("static.update.false.promote")),
        "{target}: false parents take a write-context promotion path");
    assert!(false_parent.instructions.iter().any(|inst| inst.immediate == Some(
        Immediate::RuntimeCall(RuntimeCallTarget::WriteDiagnostic))),
        "{target}: false promotion retains PHP's deprecation channel");
    assert!(false_parent.instructions.iter().any(|inst| inst.immediate == Some(
        Immediate::RuntimeCall(RuntimeCallTarget::MixedCellHasOtherOwner))),
        "{target}: handler detachment is checked without acquiring another parent owner");
    assert!(false_parent.blocks.iter().any(|block| block.name.starts_with("static.update.detached.null")),
        "{target}: a detached promoted cell bypasses the missing-key read");
    for name in ["falsePlainWrite", "falsePlainAppend", "falseLocalWrite"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(function.blocks.iter().any(|block| block.name.starts_with("nested.false.capture")),
            "{target}: {name} captures conversion exceptions before opcode completion");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::CatchBind),
            "{target}: {name} takes the in-flight exception into owned storage");
        assert!(function.instructions.iter().any(|inst| inst.immediate == Some(
            Immediate::RuntimeCall(RuntimeCallTarget::ExceptionChain))),
            "{target}: {name} preserves callback exception priority on completion errors");
    }
    for name in ["variableStringKey", "promotedStringKey", "promotedMixedKey", "promotedNullableKey"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(function.instructions.iter().any(|inst| inst.op == Op::ZeroLocalSlot),
            "{target}: {name} retires snapshots without widening string storage");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::HashGetForWrite),
            "{target}: {name} obtains the stored string-key parent instead of index zero");
    }
    for name in ["flatStringKey", "flatMixedKey", "flatNullableKey"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(function.instructions.iter().any(|inst| inst.op == Op::LoadStaticPropertyRefCell),
            "{target}: {name} publishes key-dependent container replacements through the static slot");
        let write = function.instructions.iter().find(|inst| inst.op == Op::HashSet)
            .unwrap_or_else(|| panic!("{target}: {name} must use the relocated hash root"));
        assert_eq!(write.immediate, Some(Immediate::Bool(true)),
            "{target}: {name} reuses its read's key diagnosis");
        if name != "flatStringKey" {
            let key = &function.values[write.operands[1].as_raw() as usize];
            assert_ne!(key.php_type.codegen_repr(), PhpType::Int,
                "{target}: {name} preserves mixed/null keys instead of coercing to index zero");
        }
    }
    let float = module.functions.iter().find(|function| function.name == "assocFloatKey").unwrap();
    assert!(float.instructions.iter().any(|inst| {
        inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed))
    }), "{target}: associative float parents reuse their first key diagnosis");
    for name in ["objectLiteralUpdate", "objectComputedUpdate", "objectPostfixUpdate"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let object = function.blocks.iter().filter(|block| {
            block.name.starts_with("static.update.object.parent")
        }).collect::<Vec<_>>();
        assert!(!object.is_empty(), "{target}: {name} retains the first object-offset read");
        for block in object {
            assert!(!block.instructions.iter().any(|id| {
                function.instructions[id.as_raw() as usize].immediate == Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed))
            }), "{target}: {name} must not call offsetGet again in the object arm");
        }
        assert!(function.blocks.iter().any(|block| {
            block.name.starts_with("static.update.array.parent")
                && block.instructions.iter().any(|id| {
                    function.instructions[id.as_raw() as usize].immediate == Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed))
                })
        }), "{target}: {name} preserves the already-diagnosed array autovivification arm");
    }
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
