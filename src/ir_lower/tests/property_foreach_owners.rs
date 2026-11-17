//! Purpose:
//! Verifies exceptional ownership while checking an uninitialized foreach property source.
//!
//! Called from:
//! - The AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - Mixed frame storage makes the typed receiver load an independent owned unbox.
//! - Final concrete storage keeps its borrowed receiver and needs no exceptional release.

use crate::ir::{Immediate, Op, RuntimeCallTarget};

/// Foreach initialization guards preserve owned projections and borrowed locals on every ABI.
#[test]
fn foreach_property_guards_follow_final_receiver_storage_on_all_targets() {
    let source = r#"<?php
class ForeachGuardedProperty { public array $items; }
function borrowedForeachProperty(ForeachGuardedProperty $box): void {
    try { foreach ($box->items as &$element) {} } catch (Error $error) {}
}
function projectedForeachProperty(mixed $box): void {
    $box = new ForeachGuardedProperty();
    try { foreach ($box->items as &$element) {} } catch (Error $error) {}
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for (name, owned) in [("borrowedForeachProperty", false), ("projectedForeachProperty", true)] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let probe = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized)
                .expect("typed property initialization probe");
            let receiver = function.instructions[probe].operands[0];
            let guard = function.instructions[..probe].iter().find(|inst| {
                inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
                    && inst.operands.first() == Some(&receiver)
            });
            assert_eq!(guard.is_some(), owned, "{target}: {name}: final receiver ownership");
            if let Some(guard) = guard {
                let token = guard.result.unwrap();
                assert!(function.instructions[probe + 1..].iter().any(|inst| {
                    inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionUnguardOwned))
                        && inst.operands == [token]
                }), "{target}: {name}: successful initialization retires its guard");
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
