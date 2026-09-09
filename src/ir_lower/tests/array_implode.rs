//! Purpose:
//! Pins boxed-array join normalization and owned result emission across supported targets.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Both join arities share the same typed runtime target and ownership contract.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op, Ownership, RuntimeCallTarget, RuntimeFnId};
use std::path::Path;

/// Fresh joins transfer their string owner without redundant scratch persistence at return or concat.
#[test]
fn owned_implode_results_are_not_persisted_again_at_string_boundaries() {
    let module = super::lower_source(r#"<?php
function joinOwnedResult(array $items): string { return implode(",", $items); }
function renderedTail(int $value): string { return "tail" . $value; }
function joinOwnedConcat(array $items, int $value): string {
    return implode(",", $items) . renderedTail($value);
}
echo joinOwnedResult([1, 2]), joinOwnedConcat([3, 4], $argc);
"#);
    let mut observed = 0;
    for function in &module.functions {
        for instruction in &function.instructions {
            if !matches!(instruction.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Implode))))
            {
                continue;
            }
            observed += 1;
            let joined = instruction.result.unwrap();
            assert_eq!(instruction.result_ownership, Ownership::Owned);
            assert!(!function.instructions.iter().any(|use_inst|
                use_inst.op == Op::StrPersist && use_inst.operands == [joined]), "{}", function.name);
        }
    }
    assert_eq!(observed, 2);
}

/// Declared-array joins emit normalization, exception owners and string persistence on every ABI.
#[test]
fn boxed_array_implode_normalization_is_owned_on_all_targets() {
    let source = r#"<?php
class JoinedObjectValue { public function __toString(): string { return "owned"; } }
function joinBoxedArray(array $items): string { return implode(",", $items); }
function joinOneBoxedArray(array $items): string { return join($items); }
echo joinBoxedArray(["a" => 42]), joinOneBoxedArray([true, false]);
echo joinBoxedArray(["object" => new JoinedObjectValue()]);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let joins = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|instruction| instruction.op == Op::RuntimeCall && matches!(
                instruction.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Implode)))
            )).collect::<Vec<_>>();
        assert_eq!(joins.len(), 2, "{name}");
        assert!(joins.iter().all(|instruction| instruction.result_ownership == Ownership::Owned), "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        for helper in ["__rt_mixed_unbox", "__rt_array_to_mixed", "__rt_hash_iter_next", "__rt_cleanup_call_operand_owner", "__rt_str_persist"] {
            assert!(assembly.contains(helper), "{name}: missing {helper}");
        }
        let saved_array = if name == "linux-x86_64" {
            "mov rax, QWORD PTR [rsp + 64]"
        } else {
            "ldr x0, [sp, #64]"
        };
        assert!(assembly.contains(saved_array), "{name}: normalized owner offset");
    }
}
