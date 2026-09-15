//! Purpose:
//! Verifies source-buffer and result ownership for runtime eval calls.
//!
//! Called from:
//! - AST-to-EIR unit tests on every supported target.
//!
//! Key details:
//! - Profile metadata must not hide owned eval results from expression cleanup.
//! - The source buffer has a frame root while native or eval code can throw.

use crate::ir::{Immediate, Op, Ownership};

/// Catch predicates retire their native-object adapter boxes before branching on every target.
#[test]
fn eval_catch_predicates_retire_raw_throwable_adapter_boxes_on_all_targets() {
    let source = r#"<?php
function inspectCatchPredicateOwners(string $source): void {
    try { eval($source); }
    catch (LogicException $wrong) { echo "wrong"; }
    catch (RuntimeException $right) { echo $right->getMessage(); }
}
inspectCatchPredicateOwners('throw new RuntimeException("right"); // ' . $argc);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let symbol = target.extern_symbol("__elephc_eval_object_is_a");
        let mut probes = 0;
        for path in asm.split(&format!("{symbol}\n")).skip(1) {
            let restore = if name == "linux-x86_64" { "add rsp, 96" } else { "add sp, sp, #96" };
            let cleanup = path.split_once(restore).expect("predicate scratch restoration").0;
            assert!(cleanup.contains("retire temporary eval metadata operand box"), "{name}: {cleanup}");
            assert!(cleanup.contains("__rt_decref_mixed"), "{name}: {cleanup}");
            probes += 1;
        }
        assert!(probes >= 2, "{name}: both failed and matched catch predicates need coverage");
    }
}

/// Every ABI publishes eval scope writes before propagating an exception, with bounded cleanup.
#[test]
fn eval_throw_writeback_is_bounded_before_unwinding_on_all_targets() {
    let source = r#"<?php
function catchReloadedEval(string $source): void {
    global $marker;
    $local = "before";
    try { eval($source); }
    catch (Throwable $error) { echo $local, $marker, $error->getMessage(); }
}
$marker = "old";
$source = '$local = "after"; $marker = "new"; throw new Exception("stop"); // ' . $argc;
catchReloadedEval($source);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let (_, reload) = asm.split_once("reload eval scope before propagating its pending exception")
            .expect("runtime eval scope reload");
        let (reload, finish) = reload.split_once("finish guarded eval scope writeback")
            .expect("guarded scope completion");
        assert!(reload.contains("publish eval local replacement"), "{target}: {reload}");
        assert!(reload.contains("publish eval global replacement"), "{target}: {reload}");
        assert!(reload.contains("__rt_cleanup_invoke"), "{target}: {reload}");
        let throw = finish.find("__rt_throw_current").expect("delayed native throw");
        let restored = if target == "linux-x86_64" { "add rsp, 192" } else { "add sp, sp, #192" };
        assert!(finish[..throw].contains(restored), "{target}: {finish}");
        assert_eq!(finish[..throw].matches("detach temporary call operand owner").count(), 2, "{target}");
    }
}

/// Eval reload retires displaced local owners through target-native stores and release helpers.
#[test]
fn eval_local_reload_releases_previous_owners_on_all_targets() {
    let source = r#"<?php
function reloadOwnedLocal(string $code, mixed $value): mixed {
    eval($code);
    return $value;
}
$code = '$value = "new"; // ' . $argc;
echo reloadOwnedLocal($code, "old");
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let mut replacements = 0;
        for section in asm.split("publish eval local replacement before retiring the previous owner").skip(1) {
            let (replacement, _) = section.split_once("eval local replacement owns its native payload").unwrap();
            assert!(replacement.contains("__rt_decref_mixed") || replacement.contains("__rt_heap_free_safe"), "{target}: {replacement}");
            replacements += 1;
        }
        assert!(replacements >= 2, "{target}: present and missing entries must both retire their old owners");
    }
}

/// A first syntactic store after opaque eval retires any owner restored into that future local.
#[test]
fn first_post_eval_string_store_retires_the_runtime_reloaded_slot_on_all_targets() {
    let source = r#"<?php
function opaqueEvalFutureString(string $value): string { return $value; }
function assignAfterOpaqueEval(string $source): string {
    eval($source);
    $future = opaqueEvalFutureString($source);
    echo strlen($future);
    return $future;
}
echo assignAfterOpaqueEval('return null; // ' . $argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name == "assignAfterOpaqueEval").unwrap();
        let eval = function.instructions.iter().position(|inst| {
            inst.op == Op::LanguageConstructCall
                && matches!(inst.immediate, Some(Immediate::ProfiledData { .. }))
        }).expect("opaque eval call");
        let local = function.locals.iter()
            .find(|local| local.name.as_deref() == Some("future")).unwrap();
        let slot = Some(Immediate::LocalSlot(local.id));
        let store = function.instructions.iter().position(|inst| {
            inst.op == Op::StoreLocal && inst.immediate == slot
        }).expect("first future-local store");
        assert!(eval < store, "{target}: the local is first assigned after eval");
        assert_eq!(function.instructions[eval + 1..store].iter().filter(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == slot
        }).count(), 1, "{target}: retire the owner runtime eval may have restored before the first store");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A scalar first store keeps its deferred retirement when later control flow widens the slot.
#[test]
fn first_post_eval_scalar_store_retires_a_later_widened_slot_on_all_targets() {
    let source = r#"<?php
function opaqueEvalFutureMixed(string $value): mixed { return $value; }
function widenAfterOpaqueEval(string $source, bool $replace): mixed {
    eval($source);
    $future = strlen($source);
    if ($replace) {
        $future = opaqueEvalFutureMixed($source);
    }
    return $future;
}
echo widenAfterOpaqueEval('return null; // ' . $argc, $argc > 1);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name == "widenAfterOpaqueEval").unwrap();
        let eval = function.instructions.iter().position(|inst| {
            inst.op == Op::LanguageConstructCall
                && matches!(inst.immediate, Some(Immediate::ProfiledData { .. }))
        }).expect("opaque eval call");
        let local = function.locals.iter()
            .find(|local| local.name.as_deref() == Some("future")).unwrap();
        assert_eq!(local.php_type.codegen_repr(), crate::types::PhpType::Mixed, "{target}");
        let slot = Some(Immediate::LocalSlot(local.id));
        let store = function.instructions.iter().position(|inst| {
            inst.op == Op::StoreLocal && inst.immediate == slot
        }).expect("first future-local store");
        let stored = function.value(function.instructions[store].operands[0]).unwrap();
        assert_eq!(stored.php_type.codegen_repr(), crate::types::PhpType::Int, "{target}");
        assert!(eval < store, "{target}: the local is first assigned after eval");
        assert_eq!(function.instructions[eval + 1..store].iter().filter(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == slot
        }).count(), 1, "{target}: deferred retirement must survive until final slot typing");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Main's first process-variable write retires its entry owner without inserting a null initializer.
#[test]
fn process_local_first_writes_preserve_entry_initialization_on_all_targets() {
    for statement in ["$argc += 7;", "$updated = ($argc += 7);", "++$argc;", "$argc = count($argv) + 7;", "$argv = [$argv[0] . \"replacement\"];"] {
        let name = if statement.starts_with("$argv") { "argv" } else { "argc" };
        let source = format!("<?php {statement} echo $argv[0], count($argv), $argc;");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::lower_source_at_for_target(
                &source, std::path::Path::new("main.php"), std::path::Path::new("."),
                crate::codegen::platform::Target::parse(target).unwrap(),
            );
            let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
            let local = main.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            let slot = Some(Immediate::LocalSlot(local.id));
            let store = main.instructions.iter().position(|inst| inst.op == Op::StoreLocal && inst.immediate == slot).unwrap();
            if Ownership::php_type_needs_lifetime_tracking(&local.php_type) {
                assert!(main.instructions[..store].iter().any(|inst| inst.op == Op::ReleaseLocalSlot && inst.immediate == slot),
                    "{target}: {statement} must retire the entry-point owner");
            }
            let value = main.value(main.instructions[store].operands[0]).unwrap();
            if let crate::ir::ValueDef::Instruction { inst, .. } = value.def {
                assert_ne!(main.instruction(inst).unwrap().op, Op::ConstNull, "{target}: {statement}");
            }
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Runtime eval roots its code buffer and releases discarded profiled results on every target.
#[test]
fn profiled_eval_owns_its_result_and_roots_its_source_on_all_targets() {
    let source = r#"<?php
        function discard_profiled_eval(string $source): void { eval($source); }
        discard_profiled_eval("return null; // " . $argc);
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "discard_profiled_eval").unwrap();
        let index = function.instructions.iter().position(|inst| {
            inst.op == Op::LanguageConstructCall && matches!(inst.immediate, Some(Immediate::ProfiledData { .. }))
        }).unwrap();
        let call = &function.instructions[index];
        assert_eq!(call.result_ownership, Ownership::Owned, "{target}");
        let result = call.result.unwrap();
        assert!(function.instructions[index + 1..].iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [result]
        }), "{target}: discarded eval results need cleanup");
        let root = function.instructions[..index].iter().find(|inst| {
            inst.op == Op::StoreLocal && inst.operands == [call.operands[0]]
        }).expect("the source must be visible to exceptional frame cleanup");
        assert!(function.instructions[index + 1..].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == root.immediate
        }), "{target}: normal return must clear and retire the source root");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
