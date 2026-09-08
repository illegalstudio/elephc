//! Purpose:
//! Verifies expression consumers release fresh cells but preserve borrowed scope cells.
//!
//! Called from:
//! - Magician's focused interpreter test harness.
//!
//! Key details:
//! - Fake runtime release records detect duplicate cleanup and error-path omissions.

use super::super::*;
use super::support::*;

/// Program and callable return modes differ and restore the surrounding lexical mode.
#[test]
fn expression_return_ownership_mode_is_lexical_and_restored() {
    for owned_returns in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.replace_owned_program_returns(!owned_returns);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let value = values.string("held").unwrap();
        scope.set("input", value, ScopeCellOwnership::Owned);
        let program = parse_fragment(b"return $input;").unwrap();
        let result = execute_statements_with_return_ownership(program.statements(), &mut context,
            &mut scope, &mut values, owned_returns).unwrap();
        assert!(matches!(result, EvalControl::Return(result)
            if result.value == value && result.owned == owned_returns));
        assert_eq!(context.owned_program_returns(), !owned_returns);
    }
}

/// Finally discards an owning return temporary without releasing a borrowed scope value.
#[test]
fn expression_return_ownership_finally_releases_only_owned_values() {
    for owned in [false, true] {
        let mut values = FakeOps::default();
        let value = values.string("pending").unwrap();
        release_overridden_control(EvalControl::Return(EvalExprResult { value, owned }),
            &mut values).unwrap();
        assert_eq!(values.releases.iter().filter(|released| **released == value).count(),
            usize::from(owned));
    }
}

/// Echo releases a native result whose selected signature guarantees a fresh scalar box.
#[test]
fn expression_result_ownership_releases_native_echo_result() {
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(0);
    signature.set_return_type(EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false));
    assert!(context.define_native_method_signature("KnownClass", "answer", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.new_object("KnownClass").unwrap();
    scope.set("box", object, ScopeCellOwnership::Owned);
    let program = parse_fragment(b"echo $box->answer();").unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "42");
    assert_eq!(values.releases.iter().filter(|handle| values.get(**handle) == FakeValue::Int(42)).count(), 1);
    assert!(!values.releases.contains(&object));
}

/// Compound expressions preserve the selected native result owner through branching and operators.
#[test]
fn expression_result_ownership_survives_compound_consumers() {
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(0);
    signature.set_return_type(EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false));
    assert!(context.define_native_method_signature("KnownClass", "answer", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.new_object("KnownClass").unwrap();
    scope.set("box", object, ScopeCellOwnership::Owned);
    let program = parse_fragment(br#"
echo true ? $box->answer() : 0;
echo $box->answer() ?: 0;
echo $box->answer() ?? 0;
echo $box->answer() + 1;
if ($box->answer()) { echo 'ok'; }
echo @$box->answer();
"#).unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "42424243ok42");
    assert_eq!(values.releases.iter().filter(|handle| values.get(**handle) == FakeValue::Int(42)).count(), 6);
    assert!(!values.releases.contains(&object));
}

/// Nullable native return metadata does not justify treating every returned cell as fresh.
#[test]
fn expression_result_ownership_keeps_nullable_return_unclassified() {
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(0);
    signature.set_return_type(EvalParameterType::new(vec![EvalParameterTypeVariant::Int], true));
    assert!(context.define_native_method_signature("KnownClass", "answer", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.new_object("KnownClass").unwrap();
    scope.set("box", object, ScopeCellOwnership::Owned);
    let expr = EvalExpr::MethodCall { object: Box::new(EvalExpr::LoadVar("box".into())),
        method: "answer".into(), args: vec![] };
    let result = eval_expr_result(&expr, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result.value), FakeValue::Int(42));
    assert!(!result.owned);
}

/// Returning a literal argument transfers its source owner through the expression-result carrier.
#[test]
fn expression_result_ownership_transfers_dynamic_argument_alias() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let program = parse_fragment(b"class AliasBox { public function same($value) { return $value; } } $box = new AliasBox();").unwrap();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let expr = EvalExpr::MethodCall { object: Box::new(EvalExpr::LoadVar("box".into())),
        method: "same".into(), args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String("held".into())))] };
    let result = eval_expr_result(&expr, &mut context, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result.value), FakeValue::String("held".into()));
    assert!(result.owned);
}

/// Nested arithmetic conditions release all fresh operands and keep a borrowed variable alive.
#[test]
fn expression_temporaries_release_nested_condition_owners() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let input = values.int(7).unwrap();
    scope.set("input", input, ScopeCellOwnership::Owned);
    let expression = EvalExpr::Binary {
        op: EvalBinOp::Lt,
        left: Box::new(EvalExpr::Binary {
            op: EvalBinOp::Add,
            left: Box::new(EvalExpr::LoadVar("input".into())),
            right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        }),
        right: Box::new(EvalExpr::Const(EvalConst::Int(10))),
    };
    assert!(eval_condition(&expression, &mut context, &mut scope, &mut values).unwrap());
    assert_eq!(values.releases.len(), 4);
    assert!(!values.releases.contains(&input));
    for handle in &values.releases {
        assert_eq!(values.releases.iter().filter(|other| *other == handle).count(), 1);
    }
}

/// Unary numeric operations release both their operand and the synthetic zero operand.
#[test]
fn expression_temporaries_release_unary_numeric_operands() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expression = EvalExpr::Unary {
        op: EvalUnaryOp::Plus,
        expr: Box::new(EvalExpr::Unary {
            op: EvalUnaryOp::Negate,
            expr: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        }),
    };
    assert!(eval_condition(&expression, &mut context, &mut scope, &mut values).unwrap());
    assert_eq!(values.releases.len(), 5);
}

/// Short-circuit operators release their condition without evaluating the skipped call.
#[test]
fn expression_temporaries_preserve_short_circuit() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expression = EvalExpr::Binary {
        op: EvalBinOp::LogicalAnd,
        left: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
        right: Box::new(EvalExpr::Call { name: "missing_function".into(), args: vec![] }),
    };
    assert!(!eval_condition(&expression, &mut context, &mut scope, &mut values).unwrap());
    assert_eq!(values.releases.len(), 2);
}

/// Echo consumes fresh literal/operator results without releasing a borrowed variable.
#[test]
fn expression_temporaries_echo_releases_only_owned_values() {
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let borrowed = values.int(7).unwrap();
    scope.set("borrowed", borrowed, ScopeCellOwnership::Owned);
    let program = parse_fragment(br#"echo $borrowed; echo "marker"; echo 1 + 2;"#).unwrap();
    execute_program(&program, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "7marker3");
    assert_eq!(values.releases.len(), 4);
    assert!(!values.releases.contains(&borrowed));
}

/// Failure while evaluating the right operand still releases the already evaluated left owner.
#[test]
fn expression_temporaries_release_left_on_right_error() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expression = EvalExpr::Binary {
        op: EvalBinOp::Add,
        left: Box::new(EvalExpr::Const(EvalConst::Int(999))),
        right: Box::new(EvalExpr::Call { name: "missing_function".into(), args: vec![] }),
    };
    assert!(eval_condition(&expression, &mut context, &mut scope, &mut values).is_err());
    assert_eq!(values.releases.iter()
        .filter(|handle| values.get(**handle) == FakeValue::Int(999)).count(), 1);
}
