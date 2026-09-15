//! Purpose:
//! Pins boxed comparator set lowering and result ownership on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Direct, named, CUF and FCC routes use the same typed runtime operation.
//! - Result values are key-preserving boxed PHP arrays with fresh ownership.

use crate::codegen::platform::Target;
use crate::ir::{Effects, Immediate, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;
use std::path::Path;

/// Every ABI emits boxed descriptor calls with fresh non-aliasing result metadata.
#[test]
fn boxed_array_set_comparators_lower_on_every_target() {
    let source = r#"<?php
function compareTargetFalse(false $left, false $right): int { return 0; }
function targetSetComparators(array $values): void {
    $difference = array_udiff(
        $values,
        [false],
        static fn(false $left, false $right): int => 0,
    );
    $intersection = array_uintersect(
        array1: $values,
        array2: [false],
        callback: compareTargetFalse(...),
    );
    $diff = array_udiff(...);
    $intersect = array_uintersect(...);
    echo count($difference), count($intersection);
    echo count($diff($values, [false], "compareTargetFalse"));
    echo count(call_user_func("array_uintersect", $values, [false], "compareTargetFalse"));
    echo count($intersect(...["callback" => "compareTargetFalse", "array2" => [false], "array1" => $values]));
}
targetSetComparators([false, false]);
"#;
    assert_eq!(
        RuntimeFnId::ArrayUdiff.result_ownership(),
        crate::builtins::semantics::BuiltinResultOwnership::Fresh,
    );
    assert_eq!(
        RuntimeFnId::ArrayUintersect.result_ownership(),
        crate::builtins::semantics::BuiltinResultOwnership::Fresh,
    );
    let callback_barrier = Effects::all() & !(Effects::BLOCKING_IO | Effects::NETWORK_IO);
    assert_eq!(RuntimeFnId::ArrayUdiff.effects(), callback_barrier);
    assert_eq!(RuntimeFnId::ArrayUintersect.effects(), callback_barrier);
    for builtin in ["array_udiff", "array_uintersect"] {
        assert_eq!(
            crate::types::first_class_callable_builtin_sig(builtin)
                .unwrap()
                .return_type,
            PhpType::php_array(),
            "{builtin}",
        );
    }
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let calls = module
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .filter(|inst| {
                matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                        RuntimeFnId::ArrayUdiff | RuntimeFnId::ArrayUintersect,
                    )))
                        | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                            target: RuntimeFnId::ArrayUdiff | RuntimeFnId::ArrayUintersect,
                            ..
                        }))
                )
            })
            .collect::<Vec<_>>();
        assert!(calls.len() >= 4, "{name}: comparator calls must remain typed runtime operations");
        for inst in calls {
            assert_eq!(inst.operands.len(), 3, "{name}");
            assert_eq!(inst.result_php_type, PhpType::php_array(), "{name}");
            assert!(inst.effects.contains(Effects::MAY_THROW | Effects::REFCOUNT_OP), "{name}");
        }
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(asm.contains("__rt_array_udiff_uintersect"), "{name}");
        assert!(asm.contains("__rt_callable_descriptor"), "{name}");
        assert!(!asm.contains("__rt_array_find_any_all"), "{name}");
    }
}
