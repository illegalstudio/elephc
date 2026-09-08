//! Purpose:
//! Checks temporary-cell ownership in eval Core collection builders.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - FakeOps records explicit releases; native codegen tests separately verify deep freeing.
//! - Failed value creation, key creation, and insertion must abandon the partial array safely.

use super::super::*;
use super::support::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Metadata decoding releases its keys and fetched cells on both success and invalid UTF-8.
#[test]
fn metadata_array_decoder_releases_temporary_owners() {
    for invalid in [false, true] {
        let mut values = FakeOps::default();
        let value = values.string_bytes_value(if invalid { &[0xff] } else { b"NativeParent" }).unwrap();
        let array = values.alloc(FakeValue::Array(vec![value]));
        let result = crate::interpreter::builtins::eval_runtime_string_array_to_vec(array, &mut values);
        if invalid {
            assert_eq!(result, Err(EvalStatus::RuntimeFatal));
        } else {
            assert_eq!(result.unwrap(), vec!["NativeParent"]);
        }
        assert_eq!(values.releases.len(), 2);
        assert!(values.releases.contains(&value));
        assert!(!values.releases.contains(&array));
    }
}

/// Successful insertion releases temporary operands but transfers the finished array to its caller.
#[test]
fn collection_builder_releases_operands_without_releasing_finished_array() {
    let mut values = FakeOps::default();
    let mut result = EvalArrayBuilder::assoc(&mut values, 1).unwrap();
    result.string("key", |values| values.int(42)).unwrap();
    let result = result.finish();
    assert_eq!(values.values.len(), 3);
    assert_eq!(values.releases.len(), 2);
    assert!(!values.releases.contains(&result));
    assert!(matches!(values.get(result), FakeValue::Assoc(_)));
}

/// Every failure stage releases the unfinished array and any temporary operand already allocated.
#[test]
fn collection_builder_releases_partial_results_on_each_failure_stage() {
    for stage in 0..3 {
        let mut values = FakeOps::default();
        if stage == 2 { values.fail_array_set_call(0); }
        {
            let mut result = EvalArrayBuilder::indexed(&mut values, 1).unwrap();
            let status = result.entry(
                |values| if stage == 0 { Err(EvalStatus::UnsupportedConstruct) } else { values.int(42) },
                |values, _| if stage == 1 { Err(EvalStatus::UnsupportedConstruct) } else { values.int(0) },
            );
            assert_eq!(status, Err(EvalStatus::UnsupportedConstruct));
        }
        assert_eq!(values.values.len(), stage + 1);
        assert_eq!(values.releases.len(), values.values.len());
        for id in values.values.keys() {
            assert!(values.releases.iter().any(|cell| cell.as_ptr() as usize == *id));
        }
    }
}

/// Core inventories release each materialized operand, including completed nested category arrays.
#[test]
fn core_collection_builders_release_all_temporary_cells() {
    for name in ["get_defined_constants", "get_defined_functions", "get_included_files", "get_resources", "gc_status"] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let result = if name == "gc_status" {
            eval_gc_status_values_result(&[], &mut values)
        } else {
            eval_runtime_introspection_values_result(name, &[], &mut context, &mut values)
        }.unwrap();
        for id in values.values.keys() {
            if *id != result.as_ptr() as usize {
                assert!(values.releases.iter().any(|cell| cell.as_ptr() as usize == *id),
                    "{name} leaked temporary cell {id}");
            }
        }
        assert!(!values.releases.contains(&result), "{name} released its returned array");
    }
}

/// Numeric Core arguments release their cast boxes without consuming the caller's cells.
#[test]
fn core_integer_arguments_release_conversion_cells() {
    for (name, args) in [("error_reporting", vec![0]), ("debug_backtrace", vec![2, 1])] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let args = args.into_iter().map(|arg| values.int(arg).unwrap()).collect::<Vec<_>>();
        let result = eval_runtime_introspection_values_result(name, &args, &mut context, &mut values).unwrap();
        for id in values.values.keys() {
            let cell = RuntimeCellHandle::from_raw(*id as *mut crate::value::RuntimeCell);
            if cell == result || args.contains(&cell) {
                assert!(!values.releases.contains(&cell), "{name} consumed a live caller cell");
            } else {
                assert!(values.releases.contains(&cell), "{name} leaked its integer conversion");
            }
        }
    }
}
