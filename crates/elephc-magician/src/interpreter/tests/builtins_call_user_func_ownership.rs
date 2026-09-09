//! Purpose:
//! Verifies operand ownership for eval's direct call_user_func adapter.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - Fake cell counts isolate argument leases from native container allocation.
//! - Codegen GC fixtures additionally exercise the real native value runtime.

use super::super::*;
use super::support::*;

/// Successful builtin dispatch releases both the temporary callback and its temporary argument.
#[test]
fn call_user_func_releases_all_temporary_operands() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let args = [
        EvalExpr::Const(EvalConst::String("strlen".into())),
        EvalExpr::Const(EvalConst::String("temporary".into())),
    ];
    let result = eval_builtin_call_user_func(&args, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Int(9));
    assert!(!result.is_borrowed());
    values.release(result).unwrap();
    assert!(values.cell_owners.values().all(|owners| *owners == 0));
}

/// Argument evaluation and callback validation failures retire every operand evaluated before failure.
#[test]
fn call_user_func_releases_partial_operands_and_failed_dispatch() {
    for fail_during_evaluation in [false, true] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut args = vec![
            EvalExpr::Const(EvalConst::String("missingOwnershipCallback".into())),
            EvalExpr::Const(EvalConst::String("temporary".into())),
        ];
        if fail_during_evaluation {
            args.push(EvalExpr::Call { name: "missingOwnershipOperand".into(), args: vec![] });
        }
        assert!(eval_builtin_call_user_func(&args, &mut context, &mut scope, &mut values).is_err());
        for value in &values.releases {
            assert_eq!(values.cell_owners[&(value.as_ptr() as usize)], 0);
        }
        assert!(values.releases.iter().any(|value| values.get(*value) == FakeValue::String("temporary".into())));
        assert!(values.releases.iter().any(|value| values.get(*value) == FakeValue::String("missingOwnershipCallback".into())));
    }
}

/// A borrowed callback and argument preserve their scope owners after successful dispatch.
#[test]
fn call_user_func_preserves_borrowed_scope_operands() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let callback = values.string("strlen").unwrap();
    let source = values.string("stored").unwrap();
    scope.set("callback", callback, ScopeCellOwnership::Owned);
    scope.set("source", source, ScopeCellOwnership::Owned);
    let args = [EvalExpr::LoadVar("callback".into()), EvalExpr::LoadVar("source".into())];
    let result = eval_builtin_call_user_func(&args, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Int(6));
    assert_eq!(values.cell_owners[&(callback.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
    assert_eq!(values.retains, vec![callback, source]);
    assert_eq!(values.releases, vec![callback, source]);
}

/// A returned argument retains an independent owner after its operand lease and function scope end.
#[test]
fn call_user_func_returned_argument_survives_operand_cleanup() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let declaration = parse_fragment(b"function ownedIdentity($value) { return $value; }").unwrap();
    execute_program_outcome_with_context(&mut context, &declaration, &mut scope, &mut values).unwrap();
    let args = [
        EvalExpr::Const(EvalConst::String("ownedIdentity".into())),
        EvalExpr::Const(EvalConst::String("returned".into())),
    ];
    let result = eval_builtin_call_user_func(&args, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::String("returned".into()));
    assert!(!result.is_borrowed());
    assert_eq!(values.cell_owners[&(result.as_ptr() as usize)], 1);
    values.release(result).unwrap();
    assert_eq!(values.cell_owners[&(result.as_ptr() as usize)], 0);
}
