//! Purpose:
//! Pins boxed reduction lowering and optional-initial defaults on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Declared PHP arrays and first-class calls must not reach the legacy integer-only helpers.
//! - Reduction results are Mixed even when the first carry is an integer.

/// The direct, named and first-class routes share the dynamic carry runtime on every ABI.
#[test]
fn array_reduce_uses_mixed_carries_and_shared_defaults_on_all_targets() {
    use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
    use crate::types::PhpType;
    assert!(RuntimeFnId::ArrayReduce.intrinsic_effects().contains(crate::ir::Effects::MAY_THROW));
    assert!(RuntimeFnId::ArrayReduce.intrinsic_effects().contains(crate::ir::Effects::REFCOUNT_OP));
    let source = r#"<?php
function reduceValues(): array { return ["x" => 1, "y" => 2]; }
function reduceCarry(mixed $carry, mixed $item): mixed { return $carry + $item; }
echo array_reduce(reduceValues(), "reduceCarry");
echo array_reduce(array: reduceValues(), callback: "reduceCarry");
echo call_user_func("array_reduce", reduceValues(), "reduceCarry");
echo array_reduce(...[reduceValues(), "reduceCarry"]);
echo array_reduce(array: reduceValues(), initial: "prefix", callback: fn($carry, $item) => $carry . $item);
$reduce = array_reduce(...);
echo $reduce(reduceValues(), reduceCarry(...));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let mut count = 0;
        for inst in module.functions.iter().flat_map(|function| &function.instructions) {
            if matches!(inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayReduce)))
                | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::ArrayReduce, ..
                }))
            ) {
                count += 1;
                assert_eq!(inst.operands.len(), 3, "{name}: shared normalization supplies null");
                assert_eq!(inst.result_php_type, PhpType::Mixed, "{name}: carries may change PHP type");
            }
        }
        assert!(count > 0, "{name}: keep reduction in the fixture");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_array_reduce_boxed"), "{name}");
        assert!(!asm.contains("__rt_array_reduce_str"), "{name}");
    }
}
