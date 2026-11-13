//! Purpose:
//! Verifies strict aggregate element domains, ordered failures and owned eval reductions.
//!
//! Called from:
//! - Focused walker and VM tests using a prebound array to avoid bytecode array construction gaps.
//!
//! Key details:
//! - Every case verifies its original input and failed destination as well as owner balance.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Direct and indirect reductions apply domain and overflow checks in element order on both engines.
#[test]
fn strict_numeric_aggregate_domains_eval_engines_order_and_owners() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for overflow in [false, true] {
            for vm in [false, true] {
                for product in [false, true] {
                    for (items, expected, failure) in [
                        (vec![], Some(FakeValue::Int(i64::from(product))), ""),
                        (vec![FakeValue::Int(2), FakeValue::Int(3)], Some(FakeValue::Int(if product { 6 } else { 5 })), ""),
                        (vec![FakeValue::Float(2.0), FakeValue::Int(3)], Some(FakeValue::Float(if product { 6.0 } else { 5.0 })), ""),
                        (vec![FakeValue::String("2".into())], None, "TypeError"),
                        (vec![FakeValue::Bool(true)], None, "TypeError"),
                        (vec![FakeValue::Null], None, "TypeError"),
                        (vec![FakeValue::Array(Vec::new())], None, "TypeError"),
                        (vec![FakeValue::Int(i64::MAX), FakeValue::Int(2)], if overflow { None } else {
                            Some(FakeValue::Float(if product { (i64::MAX as f64) * 2.0 } else { i64::MAX as f64 })) }, "ArithmeticError"),
                        (vec![FakeValue::Int(i64::MAX), FakeValue::Int(1), FakeValue::String("bad".into())], None,
                            if !product && overflow { "ArithmeticError" } else { "TypeError" }),
                    ] {
                        for indirect in [false, true] {
                            let builtin = if product { "array_product" } else { "array_sum" };
                            let call = if indirect { format!("call_user_func('{builtin}', $items)") } else { format!("{builtin}($items)") };
                            let shapes = if vm { format!("if (false) {{ function {builtin}($array) {{}} function call_user_func($f, $array) {{}} }}") } else { String::new() };
                            let source = format!("{shapes} $result = 23; $result = {call}; return $result;");
                            let mut context = ElephcEvalContext::new();
                            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, overflow)).unwrap();
                            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                            let elements = items.iter().cloned().map(|item| values.alloc(item)).collect::<Vec<_>>();
                            let original = FakeValue::Array(elements);
                            let array = values.alloc(original.clone());
                            if let FakeValue::Array(elements) = &original {
                                // Fake arrays store arena identities without acquiring explicit cell leases.
                                for element in elements { values.release(*element).unwrap(); }
                            }
                            scope.set("items", array, crate::scope::ScopeCellOwnership::Owned);
                            let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
                            if let Some(expected) = &expected {
                                let Ok(EvalControl::Return(value)) = outcome else { panic!("{source}: {items:?}, {feature:?}, overflow={overflow}, vm={vm}"); };
                                assert_eq!(values.get(value), *expected, "{source}: {items:?}"); values.release(value).unwrap();
                            } else {
                                assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}: {items:?}, {feature:?}, overflow={overflow}, vm={vm}");
                                let error = context.take_pending_throw().expect("aggregate failure owner");
                                assert!(values.object_is_a(error, failure, false).unwrap(), "{source}: {items:?}"); values.release(error).unwrap();
                                assert_eq!(values.get(scope.visible_cell("result").unwrap()), FakeValue::Int(23));
                            }
                            assert_eq!(values.get(scope.visible_cell("items").unwrap()), original);
                            assert!(values.warnings.is_empty());
                            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {items:?}: {:?}", values.values[id]); }
                        }
                    }
                }
            }
        }
    }
}
