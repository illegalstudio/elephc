//! Purpose:
//! Keep missing array values nullable at merge boundaries while preserving typed read paths.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Inspect the actual match merge slot and read producer on every supported target.
//! - Null-receiver reads keep exactly one live unwind record per boxed receiver.

use crate::codegen::platform::Target;
use crate::ir::{Function, Immediate, LocalKind, Op};
use crate::types::PhpType;
use std::path::Path;

/// Finds the owned slot loaded by the actual match merge block.
fn merge_storage(function: &Function) -> PhpType {
    let block = function.blocks.iter().find(|block| block.name == "match.merge")
        .expect("fixture must retain a match merge block");
    let load = block.instructions.iter().filter_map(|id| function.instruction(*id))
        .find(|inst| inst.op == Op::LoadLocal)
        .expect("match merge must load its result slot");
    let Some(Immediate::LocalSlot(slot)) = load.immediate.as_ref() else {
        panic!("match load must name its result slot");
    };
    let local = &function.locals[slot.as_raw() as usize];
    assert_eq!(local.kind, LocalKind::OwnedTemp);
    local.php_type.codegen_repr()
}

/// A miss-capable object read uses nullable merge storage and retains its concrete read opcode/type.
#[test]
fn array_element_merge_keeps_null_and_typed_hit_path_on_all_targets() {
    let source = r#"<?php
class Box { public int $n = 7; }
function pick(bool $choose, int $key): void {
    $boxes = [new Box()];
    $value = match ($choose) { true => $boxes[$key], default => new Box() };
    if ($value !== null) { echo $value->n; }
}
pick($argc > 1, $argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "pick")
            .expect("fixture must lower pick");
        assert_eq!(merge_storage(function), PhpType::Mixed, "{target}");
        let read = function.instructions.iter().find(|inst| inst.op == Op::ArrayGet)
            .expect("fixture must retain its concrete indexed read");
        assert_eq!(read.result_php_type, PhpType::Object("Box".into()), "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Object-only arms without a miss-capable read keep a concrete object merge slot.
#[test]
fn non_array_object_merge_keeps_concrete_storage_on_all_targets() {
    let source = r#"<?php
class Box { public int $n = 7; }
function pick(bool $choose): void {
    $value = match ($choose) { true => new Box(), default => new Box() };
    echo $value->n;
}
pick($argc > 1);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "pick")
            .expect("fixture must lower pick");
        assert_eq!(merge_storage(function), PhpType::Object("Box".into()), "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Merge-bound scalar getters select boxed results on every supported architecture.
#[test]
fn scalar_array_merge_getters_preserve_null_on_all_targets() {
    let source = r#"<?php
function pick(bool $choose, int $index, string $key): void {
    $floats = [1.25]; $strings = [""]; $bools = ["hit" => false];
    $callbacks = ["hit" => static fn(): string => "called"];
    $a = match ($choose) { true => $floats[$index], default => 2.5 };
    $b = match ($choose) { true => $strings[$index], default => "default" };
    $c = match ($choose) { true => $bools[$key], default => true };
    $d = match ($choose) { true => $callbacks[$key], default => static fn(): string => "default" };
    var_dump($a, $b, $c, $d === null);
}
pick($argc > 1, $argc, "missing");
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "pick")
            .expect("fixture must lower pick");
        let reads = function.instructions.iter()
            .filter(|inst| matches!(inst.op, Op::ArrayGet | Op::HashGet)).collect::<Vec<_>>();
        assert_eq!(reads.len(), 4, "{target}");
        for read in reads { assert_eq!(read.result_php_type, PhpType::Mixed, "{target}"); }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Null-receiver reads never publish one boxed receiver in two live unwind records.
#[test]
fn null_receiver_read_pins_boxed_null_once_on_all_targets() {
    let source = r#"<?php
function absent(): ?array { return null; }
function reject_warning(int $level, string $message): bool { throw new Exception("warning"); }
function run_case(): void {
    set_error_handler("reject_warning");
    try { $value = absent()[0]; } catch (Exception $error) { unset($error); }
    restore_error_handler();
}
run_case();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "run_case")
            .expect("fixture must lower run_case");
        let block = function.blocks.iter().find(|block| block.name == "nullable.index.null")
            .expect("fixture must retain the null receiver arm");
        let mut stored = std::collections::HashMap::new();
        let mut live: Vec<(crate::ir::LocalSlotId, crate::ir::ValueId)> = Vec::new();
        let mut pushes = 0;
        for inst in block.instructions.iter().filter_map(|id| function.instruction(*id)) {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate.as_ref().cloned() else { continue };
            match inst.op {
                Op::StoreLocal => { stored.insert(slot, inst.operands[0]); }
                Op::PushCallOperandOwner => {
                    let value = stored[&slot];
                    assert!(live.iter().all(|(_, owned)| *owned != value),
                        "{target}: one boxed receiver has two live unwind records");
                    live.push((slot, value));
                    pushes += 1;
                }
                Op::PopCallOperandOwner => live.retain(|(owner, _)| *owner != slot),
                _ => {}
            }
        }
        assert!(pushes >= 2, "{target}: offset evaluation and the getter must both stay pinned");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
