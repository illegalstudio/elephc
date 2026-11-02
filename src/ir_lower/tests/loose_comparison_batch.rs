//! Purpose:
//! Pins PHP-aware switch comparisons and their runtime fallback on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Bool and iterable subjects must not take integer switch dispatch.

/// All supported targets retain loose equality for bool, iterable and boxed object subjects.
#[test]
fn loose_comparison_batch_emits_php_dispatch_on_all_targets() {
    use crate::ir::{Effects, Op, Terminator};
    let source = r#"<?php
function boolSwitch(bool $x): int {
    switch ($x) { case 2: return 2; case 3: return 3; default: return 0; }
}
function boolLabels(int $x): int {
    switch ($x) { case true: return 1; case false: return 0; default: return 2; }
}
function iterableSwitch(iterable $x): int {
    switch ($x) { case 1: return 1; case 2: return 2; default: return 0; }
}
function mixedSwitch(mixed $x): int {
    switch ($x) { case 1: return 1; case 2: return 2; default: return 0; }
}
echo boolSwitch(true), boolLabels(2), iterableSwitch([1]), mixedSwitch(new stdClass());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(target).unwrap();
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        for name in ["boolSwitch", "boolLabels", "iterableSwitch", "mixedSwitch"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            assert!(!function.blocks.iter().any(|b| matches!(b.terminator, Some(Terminator::Switch { .. }))),
                "{target:?}: {name} cannot dispatch by storage word");
            let comparisons: Vec<_> = function.instructions.iter().filter(|i| i.op == Op::LooseEq).collect();
            assert_eq!(comparisons.len(), 2, "{target:?}: {name}");
            assert!(comparisons.iter().all(|i| i.effects.contains(Effects::MAY_WARN | Effects::MAY_THROW)),
                "{target:?}: comparison warning handlers can throw");
        }
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_mixed_loose_eq"), "{target:?}: object fallback");
        let mut emitter = crate::codegen_support::emit::Emitter::new(target);
        crate::codegen_support::runtime::emit_runtime(&mut emitter,
            crate::codegen_support::RuntimeFeatures::all());
        let asm = emitter.output();
        assert!(asm.contains("__rt_sprintf_warn_object_numeric"), "{target:?}: object warning");
        assert!(asm.contains("__rt_mle_null_float"), "{target:?}: quiet float/null path");
    }
}
