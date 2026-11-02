//! Purpose:
//! Verifies array-edge rejection effects, operand ownership, and target-aware class messages.
//!
//! Called from:
//! - AST-to-EIR unit tests on every supported target.
//!
//! Key details:
//! - Discarding a result must not remove the runtime argument check or its catch path.
//! - Class metadata is bounds checked before the message is persisted and thrown.

use super::*;
use crate::builtins::semantics::BuiltinResultOwnership;
use crate::ir::{Effects, RuntimeFnId};

/// Every edge result is fresh, allowing an owned rejected argument to be pinned for unwind.
#[test]
fn array_edge_guards_declare_throwing_effects_and_independent_results() {
    for id in [RuntimeFnId::ArrayFirst, RuntimeFnId::ArrayLast,
        RuntimeFnId::ArrayKeyFirst, RuntimeFnId::ArrayKeyLast] {
        assert!(id.effects().contains(Effects::MAY_THROW), "{id:?}");
        assert_eq!(id.result_ownership(), BuiltinResultOwnership::Fresh, "{id:?}");
    }
}

/// Each supported target looks up the object's runtime class before persisting its error text.
#[test]
fn array_edge_guards_emit_runtime_class_messages_on_all_targets() {
    let source = r#"<?php
function edgeFirst(mixed $input): void { try { array_first($input); } catch (TypeError $e) {} }
function edgeLast(mixed $input): void { try { array_last($input); } catch (TypeError $e) {} }
function edgeKeyFirst(mixed $input): void { try { array_key_first($input); } catch (TypeError $e) {} }
function edgeKeyLast(mixed $input): void { try { array_key_last($input); } catch (TypeError $e) {} }
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."),
            Target::parse(name).unwrap());
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for function in ["edgeFirst", "edgeLast", "edgeKeyFirst", "edgeKeyLast"] {
            let body = assembly.split_once(&format!("_fn_{function}:"))
                .unwrap_or_else(|| panic!("{name}: missing {function}")).1;
            let body = body.split("\n.globl").next().unwrap();
            let bound = body.find("_class_name_count")
                .unwrap_or_else(|| panic!("{name}: {function} has no class bound: {body}"));
            let lookup = body.find("_class_name_entries").unwrap();
            let persist = body[lookup..].find("__rt_str_persist").unwrap() + lookup;
            let throw = body[persist..].find("__rt_throw_current").unwrap() + persist;
            assert!(bound < lookup && lookup < persist && persist < throw, "{name}: {function}");
        }
    }
}
