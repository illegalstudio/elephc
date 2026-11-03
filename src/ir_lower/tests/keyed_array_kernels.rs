//! Purpose:
//! Pins the fused packed-int map+sum and filter-count kernels across every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test module through `crate::ir_lower::tests`.
//!
//! Key details:
//! - The fusion only replaces the exact literal-closure shapes it can reproduce in PHP order.
//! - Each target must emit both runtime symbols the EIR calls name.

/// The static callback shapes lower to two-operand runtime calls on every supported target.
#[test]
fn keyed_array_kernels_fuse_on_all_targets() {
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    use crate::types::PhpType;
    let source = r#"<?php
$a = range(1, 100);
echo array_sum(array_map(fn($x) => $x + 1, $a));
echo count(array_filter($a, fn($x) => $x % 2 === 0));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let mut map_sum = 0;
        let mut filter_count = 0;
        for inst in module.functions.iter().flat_map(|function| &function.instructions) {
            match &inst.immediate {
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArrayMapSumAdd,
                ))) => {
                    map_sum += 1;
                    assert_eq!(inst.operands.len(), 2, "{name}");
                    assert_eq!(inst.result_php_type.codegen_repr(), PhpType::Mixed, "{name}");
                }
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArrayFilterCountMod,
                ))) => {
                    filter_count += 1;
                    assert_eq!(inst.operands.len(), 2, "{name}");
                    assert_eq!(inst.result_php_type.codegen_repr(), PhpType::Int, "{name}");
                }
                _ => {}
            }
        }
        assert_eq!(map_sum, 1, "{name}");
        assert_eq!(filter_count, 1, "{name}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_array_map_sum_add"), "{name}");
        assert!(asm.contains("__rt_array_filter_count_mod"), "{name}");
    }
}

/// Packed Mixed element predicates lower to one tag-mask runtime call per `is_*` shape.
#[test]
fn array_element_type_predicates_fuse_on_all_targets() {
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    use crate::types::PhpType;
    let source = r#"<?php
$a = [1, "x", [1], null];
$i = 1;
echo is_int($a[$i]), is_string($a[$i]), is_array($a[$i]), is_null($a[$i]);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let mut checks = 0;
        for inst in module.functions.iter().flat_map(|function| &function.instructions) {
            if let Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ArrayElementTypeCheck,
            ))) = &inst.immediate
            {
                checks += 1;
                assert_eq!(inst.operands.len(), 3, "{name}");
                assert_eq!(inst.result_php_type.codegen_repr(), PhpType::Bool, "{name}");
            }
        }
        assert_eq!(checks, 4, "{name}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_array_element_type_check"), "{name}");
    }
}

/// A declared-int map+sum lowers to a preflight, a fused fast arm, and a generic fallback arm.
#[test]
fn declared_int_map_sum_emits_preflight_and_both_arms_on_all_targets() {
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    let source = r#"<?php
$ints = range(1, $argc + 99);
echo array_sum(array_map(fn(int $x): int => $x + 1, $ints));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let mut safe = 0;
        let mut fused = 0;
        let mut generic = 0;
        for inst in module.functions.iter().flat_map(|function| &function.instructions) {
            match &inst.immediate {
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArrayMapSumAddSafe,
                ))) => safe += 1,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArrayMapSumAdd,
                ))) => fused += 1,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArraySum,
                ))) => generic += 1,
                _ => {}
            }
        }
        assert_eq!(safe, 1, "{name}");
        assert_eq!(fused, 1, "{name}");
        assert_eq!(generic, 1, "{name}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_array_map_sum_add_safe"), "{name}");
        assert!(asm.contains("__rt_array_map_sum_add"), "{name}");
    }
}
