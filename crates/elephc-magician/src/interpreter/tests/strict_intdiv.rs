//! Purpose:
//! Verifies catchable integer division domains through direct and callable eval paths.
//!
//! Called from:
//! - Focused Magician interpreter tests.
//!
//! Key details:
//! - Both execution engines preserve failed destinations and release exception operands.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Integer division transfers catchable failures without consuming its borrowed arguments.
#[test]
fn strict_intdiv_eval_walker_vm_callable_domains_and_owners() {
    for checked in [false, true] {
        for vm in [false, true] {
            for (call, class, expected, bytecode_supported) in [
                ("intdiv(7, 2)", "", 3, true),
                ("intdiv(-7, 2)", "", -3, true),
                ("intdiv(num2: 2, num1: -7)", "", -3, false),
                ("intdiv(7, 0)", "DivisionByZeroError", 0, true),
                ("intdiv($saved, -1)", "ArithmeticError", 0, true),
                ("intdiv(num2: -1, num1: $saved)", "ArithmeticError", 0, false),
                ("call_user_func('intdiv', $saved, -1)", "ArithmeticError", 0, true),
                ("call_user_func_array('intdiv', ['num1' => 7, 'num2' => 0])", "DivisionByZeroError", 0, false),
                ("$callable(7, 0)", "DivisionByZeroError", 0, false),
            ] {
                // Named calls, array literals and first-class construction use the production walker fallback.
                let first_class = call.starts_with("$callable");
                if vm && !bytecode_supported { continue; }
                let declaration = if first_class { "$callable = intdiv(...);" } else { "" };
                // The slice requires a known shape, then runtime dispatch preserves builtin precedence.
                let shapes = if vm { "if (false) { function intdiv($a, $b) {} function call_user_func($f, $a, $b) {} }" } else { "" };
                let source = format!("{shapes} $saved = -9223372036854775807 - 1; $result = 23; {declaration} $result = {call}; return $result;");
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked)).unwrap();
                let mut scope = ElephcEvalScope::new();
                let mut values = FakeOps::default();
                let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
                if class.is_empty() {
                    let Ok(EvalControl::Return(value)) = outcome else { panic!("{call}, vm={vm}"); };
                    assert_eq!(values.get(value), FakeValue::Int(expected));
                    values.release(value).unwrap();
                } else {
                    assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{call}, vm={vm}");
                    let error = context.take_pending_throw().expect("intdiv Throwable owner");
                    assert!(values.object_is_a(error, class, false).unwrap(), "{call}, vm={vm}");
                    values.release(error).unwrap();
                    assert_eq!(values.get(scope.visible_cell("result").unwrap()), FakeValue::Int(23));
                }
                assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Int(i64::MIN));
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners {
                    assert_eq!(*owners, 0, "{call}, vm={vm}, checked={checked}: {:?}", values.values[id]);
                }
            }
        }
    }
}


/// Direct expression hooks release prepared operands when either evaluation or division fails.
#[test]
fn strict_intdiv_eval_direct_hook_releases_leases_on_all_failure_paths() {
    for args in [
        [EvalExpr::Const(EvalConst::String("7".into())), EvalExpr::Const(EvalConst::Int(0))],
        [EvalExpr::Const(EvalConst::Int(i64::MIN)), EvalExpr::Const(EvalConst::Int(-1))],
        [EvalExpr::Const(EvalConst::String("7".into())), EvalExpr::DeferredDefaultError("right operand".into())],
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert!(matches!(builtins::eval_builtin_intdiv(&args, &mut context, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable)));
        let error = context.take_pending_throw().expect("direct intdiv Throwable owner");
        values.release(error).unwrap();
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
    }
}
