//! Purpose:
//! Verifies that appends follow the final boxed representation of reference-capable locals.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Boxed append borrows directly; native layouts transfer the unboxed lease on storeback.

use std::path::Path;

use crate::codegen::platform::Target;

/// Final storage normalization selects boxed or native append coherently on every target.
#[test]
fn ref_local_append_uses_final_storage_on_all_targets() {
    use crate::ir::{Immediate, Op};
    use crate::types::PhpType;
    let source = r#"<?php
function touchUnknown(&$items): void { $items[0] = 7; }
function touchArray(array &$items): void { $items[0] = 8; }
function buildUnknown(int $n): void {
    $items = [];
    for ($i = 0; $i < $n; $i++) { $items[] = $i; }
    touchUnknown($items); echo count($items);
}

function buildArray(int $n): void {
    $items = [];
    for ($i = 0; $i < $n; $i++) { $items[] = $i; }
    touchArray($items); echo count($items);
}
function buildAssoc(int $n): void {
    $items = ["keep" => 1];
    for ($i = 0; $i < $n; $i++) { $items[] = $i; }
    touchUnknown($items); echo count($items);
}
function buildMaybe(int $n, bool $run): void {
    $items = [];
    for ($i = 0; $i < $n; $i++) { $items[] = $i; }
    if ($run) { touchUnknown($items); }
    echo count($items);
}
function buildClosure(int $n): void {
    $run = function () use ($n): void {
        $items = [];
        for ($i = 0; $i < $n; $i++) { $items[] = $i; }
        if ($n < 0) { touchUnknown($items); }
        echo count($items);
    };
    $run();
}
function joinNativeAppend(int $flag): void {
    $items = ["b", "a"];
    if ($flag < 0) { touchUnknown($items); }
    $items[] = "c"; echo count($items);
}
function earlyNativeAppend(int $flag): void {
    $items = ["b", "a"];
    if ($flag < 0) { touchUnknown($items); return; }
    $items[] = "c"; echo count($items);
}

function earlyNativeKey(int $flag): void {
    $items = ["b", "a"];
    if ($flag < 0) { touchUnknown($items); return; }
    $items["k"] = 1;
    echo "key:", count($items), "\n";
}
function earlyNativeAssoc(int $flag): void {
    $items = ["keep" => 1];
    if ($flag < 0) { touchUnknown($items); return; }
    $items["k"] = 2;
    echo "assoc:", $items["keep"], "\n";
}
function earlyNativeForeach(int $flag): void {
    $items = ["b", "a"];
    if ($flag < 0) { touchUnknown($items); return; }
    foreach ($items as &$v) { $v = "z"; }
    echo "each:", $items[0], "\n";
}
function earlyNativeIndex(int $flag): void {
    $items = ["b", "a"];
    if ($flag < 0) { touchUnknown($items); return; }
    $items[0] = "z";
    echo "idx:", $items[0], "\n";
}
function earlyNativeUnset(int $flag): void {
    $items = ["keep" => 1, "drop" => 2];
    if ($flag < 0) { touchUnknown($items); return; }
    unset($items["drop"]);
    echo "unset:", count($items), "\n";
}
function earlyNativeNested(int $flag): void {
    $items = ["keep" => ["a"]];
    if ($flag < 0) { touchUnknown($items); return; }
    $items["keep"][] = "b";
    echo "nested:", count($items["keep"]), "\n";
}
function replaceAppendScalar(&$items, string $pending): string { $items = 7; return $pending; }
function appendReplaced(): void {
    $items = [1];
    try { $items[] = replaceAppendScalar($items, str_repeat("x", 400000)); } catch (Error $e) {}
}
class AppendTarget {
    public function build(int $n): void {
        $items = [];
        for ($i = 0; $i < $n; $i++) { $items[] = $i * $n; }
        $this->touch($items);
        for ($i = $n; $i < $n + 1; $i++) { $items[] = $i; }
        echo count($items);
    }
    public function touch(&$items): void { $items[0] = 9; }
}
buildUnknown($argc); buildArray($argc); buildAssoc($argc); (new AppendTarget())->build($argc);
buildMaybe($argc, $argc < 0);
buildClosure($argc);
appendReplaced();
$items = [];
for ($i = 0; $i < $argc; $i++) { $items[] = $i; }
touchUnknown($items); echo count($items);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut boxed = 0;
        let mut native = 0;
        for function in module.functions.iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
        {
            if function.name == "AppendTarget::build" {
                assert!(!function.instructions.iter().any(|inst| inst.op == Op::MixedClone),
                    "{target}: appending after reference promotion must not clone the boxed array");
                let conversion = function.instructions.iter().find(|inst| {
                    inst.op == Op::ArrayToMixed
                }).expect("the checked product widens the array element layout");
                let source = function.value(conversion.operands[0]).unwrap();
                let crate::ir::ValueDef::Instruction { block, index, inst } = source.def else {
                    panic!("{target}: conversion source must load the local");
                };
                let load = function.instruction(inst).unwrap();
                let Some(Immediate::LocalSlot(slot)) = load.immediate else {
                    panic!("{target}: conversion source must identify the local slot");
                };
                let conversion_index = function.value(conversion.result.unwrap()).unwrap().def;
                let crate::ir::ValueDef::Instruction { index: conversion_index, .. } = conversion_index else {
                    panic!("{target}: conversion must define a value");
                };
                assert!(function.block(block).unwrap().instructions
                    [index as usize + 1..conversion_index as usize]
                    .iter().any(|id| {
                        let inst = function.instruction(*id).unwrap();
                        inst.op == Op::ReleaseLocalSlot
                            && inst.immediate == Some(Immediate::LocalSlot(slot))
                    }), "{target}: release the old boxed array before converting its owned unbox");
            }
            if function.name == "appendReplaced" {
                assert!(function.instructions.iter().any(|inst| inst.op == Op::TypePredicate),
                    "{target}: RHS replacement must retain the scalar/string parent guards");
                let error = function.blocks.iter().find(|block| {
                    block.name == "nested.write.scalar.error"
                }).expect("the runtime-shaped parent needs a scalar error block");
                let pop = error.instructions.iter().position(|id| {
                    function.instruction(*id).is_some_and(|inst| inst.op == Op::PopCallOperandOwner)
                }).expect("the rejected string result must detach its unwind pin");
                let release = error.instructions.iter().position(|id| {
                    function.instruction(*id).is_some_and(|inst| inst.op == Op::Release)
                }).expect("the rejected string result must be released");
                assert!(pop < release, "{target}: detach the unwind pin before releasing its string");
            }
            for block in &function.blocks {
                for id in &block.instructions {
                    let append = function.instruction(*id).unwrap();
                    if !matches!(append.op, Op::ArrayPush | Op::HashAppend | Op::MixedArrayAppend) { continue; }
                    let value = function.value(append.operands[0]).unwrap();
                    let crate::ir::ValueDef::Instruction { inst, .. } = value.def else { continue; };
                    let load = function.instruction(inst).unwrap();
                    let Some(Immediate::LocalSlot(slot)) = load.immediate else { continue; };
                    if function.locals[slot.as_raw() as usize].php_type.codegen_repr() != PhpType::Mixed { continue; }
                    if append.op == Op::ArrayPush {
                        assert!(matches!(value.php_type.codegen_repr(), PhpType::Array(_)),
                            "{target}: native consumers retain an indexed-array view");
                        native += 1;
                    } else {
                        assert_eq!(append.op, Op::MixedArrayAppend, "{target}: boxed append uses final storage");
                        assert_eq!(value.ownership, crate::ir::Ownership::Borrowed,
                            "{target}: direct boxed append must borrow its receiver");
                        boxed += 1;
                    }
                }
            }
        }
        assert!(boxed > 0 && native > 0 && boxed + native >= 6,
            "{target}: cover boxed and native views in methods, functions, closures and main: {boxed}/{native}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A reference-backed `iterable` source reaches dynamic indexed and hash conversion with a lease.
#[test]
fn ref_local_dynamic_iterable_foreach_owns_writeback_on_all_targets() {
    use crate::ir::{Immediate, Op, ValueDef};
    use crate::types::PhpType;

    let source = r#"<?php
function keepDynamicIterable(iterable $items): iterable { return $items; }
function touchDynamicIterable(&$items): void { $items = 123; }
function mutateDynamicIterable(iterable $input, int $flag): void {
    $items = keepDynamicIterable($input);
    if ($flag < 0) {
        touchDynamicIterable($items);
        return;
    }
    foreach ($items as &$value) { $value = 1; }
    unset($value);
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name == "mutateDynamicIterable")
            .expect("dynamic iterable fixture function");
        let iter_start = function.instructions.iter()
            .find(|instruction| instruction.op == Op::IterStart)
            .expect("dynamic iterable fixture must reach IterStart");
        let source_value = function.value(iter_start.operands[0]).expect("iterator source value");
        assert_eq!(source_value.php_type.codegen_repr(), PhpType::Iterable, "{name}");
        let ValueDef::Instruction { inst: source_load, .. } = source_value.def else {
            panic!("{name}: iterator source must be loaded from the local");
        };
        let source_load = function.instruction(source_load).expect("iterator source load");
        assert_eq!(source_load.op, Op::LoadRefCell, "{name}");
        let Some(Immediate::LocalSlot(source_slot)) = source_load.immediate else {
            panic!("{name}: reference load must identify its raw local slot");
        };
        assert_eq!(
            function.locals[source_slot.as_raw() as usize].php_type.codegen_repr(),
            PhpType::Mixed,
            "{name}: the iterable view must come from final Mixed storage",
        );
        assert!(
            !function.instructions.iter().any(|instruction| instruction.op == Op::ArrayToHash),
            "{name}: iterable source must stay dynamic until backend dispatch",
        );

        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = assembly.split_once("@fn name=mutateDynamicIterable ").unwrap().1
            .split_once("@endfn name=mutateDynamicIterable").unwrap().0;
        let body = body.split_once("op=iter_start").expect("iterator assembly").1;
        let indexed_label = body.lines()
            .find(|line| line.contains("iter_start_dyn_indexed") && line.trim_end().ends_with(':'))
            .expect("dynamic indexed label");
        let hash_label = body.lines()
            .find(|line| line.contains("iter_start_dyn_hash") && line.trim_end().ends_with(':'))
            .expect("dynamic hash label");
        let object_label = body.lines()
            .find(|line| line.contains("iter_start_dyn_object") && line.trim_end().ends_with(':'))
            .expect("dynamic object label");
        let indexed_start = body.find(indexed_label).unwrap();
        let hash_start = body.find(hash_label).unwrap();
        let object_start = body.find(object_label).unwrap();
        let indexed = &body[indexed_start..hash_start];
        let hash = &body[hash_start..object_start];

        for (branch, conversion) in [
            (indexed, "__rt_array_to_hash"),
            (hash, "__rt_hash_to_mixed"),
        ] {
            let retain = branch.find("__rt_incref")
                .unwrap_or_else(|| panic!("{name}: missing borrowed-source retain in:\n{branch}"));
            let cleanup = branch.find("__rt_decref_mixed")
                .unwrap_or_else(|| panic!("{name}: missing old Mixed-box cleanup in:\n{branch}"));
            let convert = branch.find(conversion)
                .unwrap_or_else(|| panic!("{name}: missing {conversion} in:\n{branch}"));
            assert!(retain < cleanup && cleanup < convert,
                "{name}: retain and retire the raw Mixed owner before {conversion}:\n{branch}");
            let writeback = &branch[convert..];
            let boxed = writeback.find("__rt_mixed_from_value")
                .unwrap_or_else(|| panic!("{name}: converted owner must be boxed for raw-local writeback:\n{branch}"));
            let transferred = writeback.find("__rt_decref_any")
                .unwrap_or_else(|| panic!("{name}: owned writeback must transfer its retained container lease:\n{branch}"));
            assert!(boxed < transferred,
                "{name}: box the converted owner before transferring its retained lease:\n{branch}");
        }
    }
}

/// Mixed-backed nested writes clear their source slot before every consuming fetch.
#[test]
fn mixed_nested_writes_retire_source_slots_on_all_targets() {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, ValueDef};
    use crate::types::PhpType;

    let source = r#"<?php
function branchingKey(int $condition): void {
    $items = [10, 20, 30];
    $copy = $items;
    try { $items[$condition ? "1" : "0"]["x"] = 1; }
    catch (Throwable $error) {}
}
function republishedKey(int $condition): void {
    $items = [[0], [0]];
    $items["seed"] = [0];
    $copy = $items;
    $items[($items = [[0], [0]]) ? ($condition ? 1 : 0) : 0][0] = 1;
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        for function_name in ["branchingKey", "republishedKey"] {
            let function = module.functions.iter()
                .find(|function| function.name == function_name)
                .unwrap_or_else(|| panic!("{name}: missing {function_name}"));
            let mut witnessed = false;
            for block in &function.blocks {
                for (fetch_index, fetch_id) in block.instructions.iter().copied().enumerate() {
                    let fetch = function.instruction(fetch_id).unwrap();
                    if !matches!(fetch.immediate,
                        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite
                            | RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed)))
                    {
                        continue;
                    }
                    let Some(receiver) = fetch.operands.first().copied() else { continue; };
                    let Some(receiver_value) = function.value(receiver) else { continue; };
                    let ValueDef::Instruction { inst: load_id, .. } = receiver_value.def else {
                        continue;
                    };
                    let Some(load_index) = block.instructions.iter().position(|id| *id == load_id)
                    else {
                        continue;
                    };
                    let load = function.instruction(load_id).unwrap();
                    if load.op != Op::LoadLocal {
                        continue;
                    }
                    let Some(Immediate::LocalSlot(slot)) = load.immediate else { continue; };
                    if !matches!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(),
                        PhpType::Mixed | PhpType::Union(_))
                    {
                        continue;
                    }
                    assert!(block.instructions[load_index + 1..fetch_index].iter().any(|id| {
                        let inst = function.instruction(*id).unwrap();
                        inst.op == Op::ReleaseLocalSlot
                            && inst.immediate == Some(Immediate::LocalSlot(slot))
                    }), "{name}: {function_name} must clear the Mixed source slot before fetch");
                    witnessed = true;
                }
            }
            assert!(witnessed, "{name}: {function_name} must cover a Mixed-backed concrete fetch");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A string view from the raw side of a conditional reference promotion borrows Mixed payload bytes.
#[test]
fn ref_local_mixed_string_reads_borrow_on_all_targets() {
    use crate::ir::{Op, Ownership};
    use crate::types::PhpType;

    let source = r#"<?php
function replace_ref_string(&$value): void { $value = 123; }
function read_unpromoted_ref_string(int $flag): void {
    $text = str_repeat("Z", 64);
    if ($flag < 0) { replace_ref_string($text); return; }
    echo $text;
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name == "read_unpromoted_ref_string")
            .expect("fixture function");
        let load = function.instructions.iter().find(|instruction| {
            instruction.op == Op::LoadRefCell
                && instruction.result.is_some_and(|result| {
                    function.value(result).is_some_and(|value| {
                        value.php_type.codegen_repr() == PhpType::Str
                    })
                })
        }).expect("surviving branch reads the conditionally promoted string local");
        assert_eq!(
            function.value(load.result.unwrap()).unwrap().ownership,
            Ownership::Borrowed,
            "{name}: the storage-backed string view stays borrowed",
        );

        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = assembly.split_once("@fn name=read_unpromoted_ref_string ").unwrap().1
            .split_once("@endfn name=read_unpromoted_ref_string").unwrap().0;
        let read = body.split_once("op=load_ref_cell").unwrap().1
            .split_once("op=echo_value").unwrap().0;
        assert!(read.contains("__rt_mixed_unbox"), "{name}: {read}");
        assert!(!read.contains("__rt_mixed_cast_string"), "{name}: {read}");
    }
}
