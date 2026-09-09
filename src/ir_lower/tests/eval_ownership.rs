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
