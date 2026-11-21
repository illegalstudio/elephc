//! Purpose:
//! Verifies numeric builtin conversion profiles through eval direct and callable dispatch.
//!
//! Called from:
//! - Focused walker/VM semantic and ownership tests.
//!
//! Key details:
//! - Complete decimal and radix strings share the neutral checked kernels.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Eval builtin calls keep exact values across named, unpacked and first-class callable syntax.
#[test]
fn strict_builtin_casts_eval_walker_vm_values_and_owners() {
    for vm in [false, true] {
        for (source, expected, shape) in [("return intval('9223372036854775807.9');", FakeValue::Int(i64::MAX), Some(("intval", 1))),
            ("return intval('-8000000000000000', 16);", FakeValue::Int(i64::MIN), Some(("intval", 2))),
            ("return intval(value: 'ff', base: 16);", FakeValue::Int(255), None),
            ("$cast = intval(...); return $cast('ff', 16);", FakeValue::Int(255), None),
            ("return intval(7.9, 1);", FakeValue::Int(7), Some(("intval", 2))),
            ("return floatval('1e999');", FakeValue::Float(f64::INFINITY), Some(("floatval", 1))),
            ("$cast = floatval(...); return $cast('1.5');", FakeValue::Float(1.5), None)] {
            if vm && shape.is_none() { continue; }
            let source = shaped_source(source, vm, shape);
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
            let Ok(EvalControl::Return(result)) = outcome else { panic!("{source} vm={vm}"); };
            assert_eq!(values.get(result), expected, "{source} vm={vm}"); values.release(result).unwrap();
            for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
        }
    }
}

/// Rejected base strings, ranges and original aggregate kinds transfer exactly one Throwable owner.
#[test]
fn strict_builtin_casts_eval_failures_leave_no_argument_or_throwable_owners() {
    for vm in [false, true] {
        for (source, shape) in [("return intval('junk');", Some(("intval", 1))),
            ("return intval('8000000000000000', 16);", Some(("intval", 2))),
            ("return intval('ff', 1);", Some(("intval", 2))), ("return intval('fftail', 16);", Some(("intval", 2))),
            ("return floatval('junk');", Some(("floatval", 1))),
            ("$cast = intval(...); return $cast('8000000000000000', 16);", None),
            ("$cast = floatval(...); return $cast('junk');", None)] {
            if vm && shape.is_none() { continue; }
            let source = shaped_source(source, vm, shape);
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(matches!(staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values), Err(EvalStatus::UncaughtThrowable)), "{source} vm={vm}");
            let error = context.take_pending_throw().unwrap(); assert!(values.object_is_a(error, "ValueError", false).unwrap());
            values.release(error).unwrap();
            for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
        }
    }
}

/// Admits a positional call shape to the VM without changing builtin precedence at execution.
fn shaped_source(source: &str, vm: bool, shape: Option<(&str, usize)>) -> String {
    if !vm { return source.to_string(); }
    let (name, arity) = shape.expect("this source has a bytecode-supported positional shape");
    let params = (0..arity).map(|index| format!("$arg{index}")).collect::<Vec<_>>().join(", ");
    format!("if (false) {{ function {name}({params}) {{}} }} {source}")
}
