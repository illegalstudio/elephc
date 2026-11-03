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
function referenceSwitch(): int {
    $n = 2;
    set_error_handler(function(int $level, string $message) use (&$n): bool { $n = 1; return true; });
    switch ($n) { case new stdClass(): $result = 1; break; case 2: $result = 2; break; default: $result = 0; }
    restore_error_handler();
    return $result;
}
function callableNumber(callable $callback): bool { return $callback == 1; }
echo boolSwitch(true), boolLabels(2), iterableSwitch([1]), mixedSwitch(new stdClass()), referenceSwitch(),
    callableNumber("strlen"), callableNumber(callback: "strlen"), callableNumber(strlen(...));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(target).unwrap();
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let constructors: Vec<_> = module.functions.iter().flat_map(|f| &f.instructions).map(|i| i.op).collect();
        assert_eq!(constructors.iter().filter(|op| **op == Op::CallableValueNew).count(), 2,
            "{target:?}: positional and named implicit string callables");
        assert!(constructors.contains(&Op::FirstClassCallableNew), "{target:?}: actual Closure object");
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
        assert!(asm.contains("__rt_mixed_loose_eq_refs"), "{target:?}: live reference fallback");
        let mut emitter = crate::codegen_support::emit::Emitter::new(target);
        crate::codegen_support::runtime::emit_runtime(&mut emitter,
            crate::codegen_support::RuntimeFeatures::all());
        let asm = emitter.output();
        assert!(asm.contains("__rt_mixed_loose_eq_refs"), "{target:?}: live reference runtime entry");
        let entry = asm.split("__rt_mixed_loose_eq_refs:").nth(1).unwrap();
        let entry = entry.split("__rt_mixed_loose_eq_common:").next().unwrap();
        assert!(entry.lines().any(|line| {
            let line = line.trim_start();
            (line.starts_with("b ") || line.starts_with("jmp "))
                && line.contains("__rt_mixed_loose_eq_common")
        }), "{target:?}: a separate global entry must branch across section boundaries");
        assert!(asm.contains("__rt_sprintf_warn_object_numeric"), "{target:?}: object warning");
        assert!(asm.contains("__rt_mle_null_float"), "{target:?}: quiet float/null path");
    }
}
