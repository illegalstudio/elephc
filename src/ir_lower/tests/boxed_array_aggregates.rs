//! Purpose:
//! Pins storage-neutral numeric aggregates and warning effects on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Direct, named and callable results always use independently owned Mixed storage.

/// Every target preserves int-or-float results and warning-handler effects through typed EIR.
#[test]
fn array_aggregates_use_boxed_results_and_observable_effects_on_all_targets() {
    use crate::ir::{Effects, Immediate, RuntimeCallTarget, RuntimeFnId};
    use crate::types::PhpType;
    for target in [RuntimeFnId::ArraySum, RuntimeFnId::ArrayProduct] {
        assert!(target.effects().contains(Effects::MAY_WARN | Effects::MAY_THROW | Effects::WRITES_GLOBAL | Effects::REFCOUNT_OP));
        assert!(target.runtime_callable_supported());
        assert!(target.callable_accepts(Some(&PhpType::Mixed)));
    }
    let source = r#"<?php
function aggregateValues(): array { return ["x" => 1.5, "y" => "2.25"]; }
echo array_sum(aggregateValues());
echo array_product(array: aggregateValues());
$sum = array_sum(...);
echo $sum(aggregateValues());
echo call_user_func("array_product", aggregateValues());
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let mut count = 0;
        for inst in module.functions.iter().flat_map(|function| &function.instructions) {
            if matches!(inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct)))
                | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct, ..
                }))
            ) {
                count += 1;
                assert_eq!(inst.operands.len(), 1, "{name}");
                assert_eq!(inst.result_php_type.codegen_repr(), PhpType::Mixed, "{name}");
            }
        }
        assert!(count >= 2, "{name}: keep both aggregators in the fixture");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_array_sum_boxed"), "{name}");
        assert!(asm.contains("__rt_array_product_boxed"), "{name}");
        assert!(!asm.contains("__rt_hash_sum_mixed"), "{name}");
    }
}
