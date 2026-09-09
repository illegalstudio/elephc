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
