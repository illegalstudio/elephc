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
