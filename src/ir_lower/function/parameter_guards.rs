//! Purpose:
//! Builds reusable EIR guards for public callable scalar parameter contracts.
//!
//! Called from:
//! - Final descriptor helper emission after all runtime signatures are known.
//!
//! Key details:
//! - The input is borrowed by the invoker and receives ordinary Mixed parameter ownership.
//! - Physical implementation syntax does not weaken the public callable boundary.

use super::*;
use elephc_semantics::conversions::ValueKind;

/// Lowers the same exact scalar kind contract used by direct argument boundaries.
pub(crate) fn lower_callable_parameter_guard(module: &mut Module, label: &str, expected: &PhpType) -> Function {
    let span = Span::dummy();
    let value = Expr::new(ExprKind::Variable("value".into()), span);
    let allowed = crate::semantics::conversions::scalar_contract(expected).expect("scalar guard contract");
    let false_only = crate::semantics::conversions::only_false(expected);
    let mut alternatives = Vec::new();
    for (kind, name) in [(ValueKind::Integer, "is_int"), (ValueKind::Float, "is_float"),
        (ValueKind::Boolean, "is_bool"), (ValueKind::String, "is_string"), (ValueKind::Null, "is_null")] {
        if !allowed.allows(kind) { continue; }
        alternatives.push(if false_only && kind == ValueKind::Boolean {
            Expr::new(ExprKind::BinaryOp { left: Box::new(value.clone()), op: BinOp::StrictEq,
                right: Box::new(Expr::new(ExprKind::BoolLiteral(false), span)) }, span)
        } else {
            Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args: vec![value.clone()] }, span)
        });
    }
    let accepted = alternatives.into_iter().reduce(|left, right| Expr::new(ExprKind::BinaryOp {
        left: Box::new(left), op: BinOp::Or, right: Box::new(right),
    }, span)).expect("a scalar contract accepts at least one kind");
    let guard = Stmt::new(StmtKind::If {
        condition: accepted, then_body: Vec::new(), elseif_clauses: Vec::new(),
        else_body: Some(vec![Stmt::new(StmtKind::Throw(Expr::new(ExprKind::NewObject {
            class_name: Name::unqualified("TypeError"), args: vec![Expr::new(ExprKind::StringLiteral(
                format!("--strict-coercions requires {expected} without implicit conversion")), span)],
        }, span)), span)]),
    }, span);
    let signature = FunctionSig {
        params: vec![("value".into(), PhpType::Mixed)], param_type_exprs: vec![None],
        param_attributes: vec![Vec::new()], defaults: vec![None], return_type: PhpType::Void,
        declared_return: true, by_ref_return: false, ref_params: vec![false], declared_params: vec![false],
        variadic: None, deprecation: None, is_generator: false,
    };
    lower_operator_callable_body(module, label, &signature, crate::source::SourceMode::Internal, vec![guard])
}
