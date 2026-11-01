//! Purpose:
//! Pins the ownership of local-load views used as closure captures and by-reference arrays.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Each regression lowers and emits for all five supported targets.
//! - Borrowed views must not release; owned box and unbox conversions must release.

use crate::ir::{Function, Immediate, LocalKind, Op};
use crate::codegen::platform::Target;
use std::path::Path;

const TARGETS: [&str; 5] = [
    "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
];

/// A recursive capture keeps its boxed cell ABI while the closure body sees a callable view.
#[test]
fn recursive_by_ref_capture_separates_physical_and_logical_types() {
    let source = r#"<?php
$callback = function (int $value) use (&$callback): int {
    return $value <= 1 ? 1 : $value * $callback($value - 1);
};
echo $callback(5);
"#;
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let closure = module.closures.first().unwrap_or_else(|| panic!("{target}: missing closure"));
        let capture = closure.params.iter().find(|param| param.name == "callback")
            .unwrap_or_else(|| panic!("{target}: missing capture parameter"));
        assert!(capture.by_ref, "{target}: recursive capture must remain by-reference");
        assert_eq!(capture.php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}: hidden ABI parameter");
        let local = closure.locals.iter().find(|local| local.name.as_deref() == Some("callback"))
            .unwrap_or_else(|| panic!("{target}: missing logical capture local"));
        assert_eq!(local.php_type.codegen_repr(), crate::types::PhpType::Callable, "{target}: body view");
        let main = module.functions.iter().find(|function| function.name == "main")
            .unwrap_or_else(|| panic!("{target}: missing main function"));
        let callback = main.locals.iter().find(|local| local.name.as_deref() == Some("callback"))
            .unwrap_or_else(|| panic!("{target}: missing callback local"));
        let store = main.instructions.iter().find(|instruction| {
            instruction.op == Op::StoreRefCell
                && instruction.immediate == Some(Immediate::LocalSlot(callback.id))
        }).unwrap_or_else(|| panic!("{target}: missing recursive callback store"));
        assert_eq!(
            store.result_php_type.codegen_repr(),
            crate::types::PhpType::Mixed,
            "{target}: recursive assignment must preserve the capture cell payload ABI",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Object-reference closure descriptors preserve the concrete payload ABI used by their body.
#[test]
fn closure_object_reference_keeps_its_concrete_payload_abi() {
    let source = r#"<?php
interface ClosureObjectReferenceContract { public function label(): string; }
$callback = function (ClosureObjectReferenceContract &$value): string {
    return $value->label();
};
"#;
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(target).unwrap(),
        );
        let closure = module
            .closures
            .first()
            .unwrap_or_else(|| panic!("{target}: missing closure"));
        let parameter = closure
            .params
            .first()
            .unwrap_or_else(|| panic!("{target}: missing closure parameter"));
        assert!(parameter.by_ref, "{target}: parameter must remain by-reference");
        assert_eq!(
            parameter.php_type.codegen_repr(),
            crate::types::PhpType::Object("ClosureObjectReferenceContract".to_string()),
            "{target}: the callee must receive the concrete object payload slot",
        );
        let signature = closure
            .signature
            .as_ref()
            .unwrap_or_else(|| panic!("{target}: missing closure signature"));
        assert_eq!(
            signature.params[0].1.codegen_repr(),
            parameter.php_type.codegen_repr(),
            "{target}: descriptor and callee payload layouts must agree",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Named object references keep a boxed entry ABI while the body reads a concrete object view.
#[test]
fn named_object_reference_separates_physical_and_logical_types() {
    let source = r#"<?php
interface NamedObjectReferenceContract { public function label(): string; }
function readNamedObjectReference(NamedObjectReferenceContract &$value): string {
    $label = $value->label();
    $value = "changed";
    return $label;
}
"#;
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(target).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "readNamedObjectReference")
            .unwrap_or_else(|| panic!("{target}: missing named function"));
        let parameter = function
            .params
            .first()
            .unwrap_or_else(|| panic!("{target}: missing function parameter"));
        assert!(parameter.by_ref, "{target}: parameter must remain by-reference");
        assert_eq!(
            parameter.php_type.codegen_repr(),
            crate::types::PhpType::Mixed,
            "{target}: the runtime entry keeps its boxed reference ABI",
        );
        let local = function
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some("value"))
            .unwrap_or_else(|| panic!("{target}: missing logical parameter local"));
        assert_eq!(
            local.php_type.codegen_repr(),
            crate::types::PhpType::Mixed,
            "{target}: the local must retain the boxed ref-cell payload ABI",
        );
        let loads = function
            .instructions
            .iter()
            .filter(|instruction| instruction.op == Op::LoadRefCell)
            .collect::<Vec<_>>();
        assert!(!loads.is_empty(), "{target}: missing reference-cell loads");
        assert!(
            loads.iter().all(|load| {
                load.result_php_type.codegen_repr()
                    == crate::types::PhpType::Object(
                        "NamedObjectReferenceContract".to_string(),
                    )
            }),
            "{target}: every body read must expose the concrete object view",
        );
        let store = function
            .instructions
            .iter()
            .find(|instruction| {
                instruction.op == Op::StoreRefCell
                    && instruction.immediate == Some(Immediate::LocalSlot(local.id))
            })
            .unwrap_or_else(|| panic!("{target}: missing replacement ref-cell store"));
        assert_eq!(
            store.result_php_type.codegen_repr(),
            crate::types::PhpType::Mixed,
            "{target}: replacement stores must follow the boxed physical payload ABI",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A hidden Mixed owner does not change the string payload ABI of its returned ref-cell.
#[test]
fn bound_string_reference_store_keeps_its_physical_payload_type() {
    let source = r#"<?php
class BoundStringReferenceHost { public string $value = 'initial'; }
$host = new BoundStringReferenceHost();
$bound = \Closure::bind(fn &() => $this->value, $host, $host);
$reference = &$bound();
$reference = 'changed';
echo $host->value;
"#;
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(target).unwrap(),
        );
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap_or_else(|| panic!("{target}: missing main function"));
        let reference = main
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some("reference"))
            .unwrap_or_else(|| panic!("{target}: missing reference local"));
        let store = main
            .instructions
            .iter()
            .find(|instruction| {
                instruction.op == Op::StoreRefCell
                    && instruction.immediate == Some(Immediate::LocalSlot(reference.id))
            })
            .unwrap_or_else(|| panic!("{target}: missing string reference store"));
        assert_eq!(
            store.result_php_type.codegen_repr(),
            crate::types::PhpType::Str,
            "{target}: the cell payload remains a raw string despite its hidden Mixed owner",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Rebindable method closures keep their boxed receiver type in both ABI and body storage.
#[test]
fn rebindable_this_capture_separates_body_storage_from_signature_inference() {
    let source = r#"<?php
class RebindableCaptureHost {
    public int $value = 42;
    public function reader(): callable {
        return function (): int { return $this->value; };
    }
}
echo (new RebindableCaptureHost())->reader()();
"#;
    for target in TARGETS {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let closure = module.closures.first().unwrap_or_else(|| panic!("{target}: missing closure"));
        let capture = closure.params.iter().find(|param| param.name == "this")
            .unwrap_or_else(|| panic!("{target}: missing receiver capture parameter"));
        assert_eq!(capture.php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}: hidden ABI parameter");
        let local = closure.locals.iter().find(|local| local.name.as_deref() == Some("this"))
            .unwrap_or_else(|| panic!("{target}: missing receiver capture local"));
        assert_eq!(local.php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}: body storage");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Alternative captures each publish the same fallback owner before their closure is built.
#[test]
fn alternative_mutating_captures_publish_cell_owners_on_every_target() {
    let source = r#"<?php
class AlternativeCaptureOwner { public string $name = 'old'; }
function alternativeCaptureOwners(int $mode): void {
    $item = new AlternativeCaptureOwner();
    if ($mode === 0) {
        $change = function() use (&$item): int { $item = new AlternativeCaptureOwner(); return 1; };
    } else {
        $change = function() use (&$item): int { $item = new AlternativeCaptureOwner(); return 2; };
    }
    echo $change();
}
alternativeCaptureOwners($argc);
"#;
    for target in TARGETS {
        let (module, function) = lower_function(source, target, "alternativeCaptureOwners");
        let item = function.locals.iter().find(|local| local.name.as_deref() == Some("item")).unwrap();
        let owners = function.instructions.iter().filter_map(|inst| {
            if inst.op != Op::PromoteLocalRefCell { return None; }
            match inst.immediate {
                Some(Immediate::LocalSlotPair { first, second }) if first == item.id => Some(second),
                _ => None,
            }
        }).collect::<Vec<_>>();
        assert_eq!(owners.len(), 2, "{target}: both alternatives must promote their raw path");
        assert_eq!(owners[0], owners[1], "{target}: alternatives share a single owner slot");
        assert_eq!(function.locals[owners[0].as_raw() as usize].kind, LocalKind::RefCell, "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Lowers one source and returns the named function beside its complete module.
fn lower_function(source: &str, target: &str, name: &str) -> (crate::ir::Module, Function) {
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
    );
    let function = module.functions.iter()
        .find(|function| function.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{target}: missing {name}"))
        .clone();
    (module, function)
}

/// By-reference array normalization retires the concrete view after boxing and before storage.
#[test]
fn by_ref_array_promotion_releases_its_owned_view_on_every_target() {
    let source = r#"<?php
class RefArrayPayload { public function __destruct() {} }
function replaceRefArray(array &$target): void { $target = []; }
function promoteCallArray(): void {
    $values = [new RefArrayPayload()];
    replaceRefArray($values);
}
promoteCallArray();
"#;
    for target in TARGETS {
        let (module, function) = lower_function(source, target, "promoteCallArray");
        let (boxing, value, slot) = function.instructions.iter().enumerate()
            .find_map(|(index, inst)| {
                if inst.op != Op::MixedBox { return None; }
                let value = *inst.operands.first()?;
                let load = function.instructions.iter().find(|load| load.result == Some(value))?;
                let Some(Immediate::LocalSlot(slot)) = load.immediate else { return None; };
                (load.op == Op::LoadLocal).then_some((index, value, slot))
            }).expect("array argument normalization boxes a concrete local view");
        let release = function.instructions.iter().enumerate()
            .find_map(|(index, inst)| {
                (index > boxing && inst.op == Op::Release && inst.operands == [value])
                    .then_some(index)
            }).expect("the owned concrete view must retire after boxing");
        let store = function.instructions.iter().enumerate()
            .find_map(|(index, inst)| {
                (index > boxing && inst.op == Op::StoreLocal
                    && inst.immediate == Some(Immediate::LocalSlot(slot))).then_some(index)
            }).expect("the normalized value replaces its source local");
        assert!(boxing < release && release < store, "{target}");
        assert_eq!(function.instructions.iter().filter(|inst| {
            inst.op == Op::Release && inst.operands == [value]
        }).count(), 1, "{target}: box_value_as_mixed already retires the owned input view");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Only an owning capture load keeps its provisional release, on every supported target.
///
/// Three by-value captures cover the whole matrix in one lowering: `$handler` is a callable
/// slot read as a callable and `$counter` is a Mixed slot read as Mixed, both borrowed views
/// whose release would cancel the retain `closure_new` takes, while `$items` is read as a
/// concrete array from a slot the later by-reference call widens to PHP `array`, so its
/// unbox-and-retain release has to survive. Each target also emits, because the capture store
/// sequence is what consumes this metadata.
#[test]
fn borrowed_by_value_capture_views_are_pruned_on_every_target() {
    let source = r#"<?php
function dropCaptureArray(array &$slot): void { $slot = ['changed']; }
function buildCaptureOwners(int $seed, callable $callback): callable {
    $handler = $callback;
    $counter = $seed + 3;
    $items = ['kept'];
    $closure = function () use ($handler, $counter, $items): string {
        return $counter . ':' . $items[0];
    };
    dropCaptureArray($items);
    return $closure;
}
echo 'built';
"#;
    for target in TARGETS {
        let (module, function) = lower_function(source, target, "buildCaptureOwners");
        let closure_new = function
            .instructions
            .iter()
            .find(|inst| inst.op == Op::ClosureNew)
            .unwrap_or_else(|| panic!("{target}: the closure literal lowers to closure_new"));
        let mut seen = 0usize;
        for operand in &closure_new.operands {
            let load = function
                .instructions
                .iter()
                .find(|inst| inst.result == Some(*operand))
                .unwrap_or_else(|| panic!("{target}: every capture operand has a definition"));
            let Some(Immediate::LocalSlot(slot)) = load.immediate else {
                panic!("{target}: every by-value capture loads a local slot");
            };
            assert_eq!(load.op, Op::LoadLocal, "{target}");
            assert_eq!(
                function.locals[slot.as_raw() as usize].kind,
                LocalKind::PhpLocal,
                "{target}",
            );
            let storage = function.locals[slot.as_raw() as usize].php_type.clone();
            let result = function.value(*operand).unwrap().php_type.clone();
            let released = function
                .instructions
                .iter()
                .any(|inst| inst.op == Op::Release && inst.operands == [*operand]);
            let owns = crate::ir::local_load_coercion_owns_result(&storage, &result);
            assert_eq!(released, owns, "{target}: {storage:?} -> {result:?}");
            match (storage.codegen_repr(), result.codegen_repr()) {
                (crate::types::PhpType::Callable, crate::types::PhpType::Callable) => {
                    assert!(!released, "{target}: a borrowed callable capture keeps no release");
                    seen += 1;
                }
                (crate::types::PhpType::Mixed, crate::types::PhpType::Mixed) => {
                    assert!(!released, "{target}: a borrowed Mixed capture keeps no release");
                    seen += 1;
                }
                (crate::types::PhpType::Mixed, crate::types::PhpType::Array(_)) => {
                    assert!(released, "{target}: an unboxed array capture keeps its release");
                    seen += 1;
                }
                other => panic!("{target}: unexpected capture shape {other:?}"),
            }
        }
        assert_eq!(seen, 3, "{target}: all three capture shapes are lowered");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
