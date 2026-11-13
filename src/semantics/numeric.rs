//! Purpose:
//! Adapts checked numeric policy to compiler-owned source and type metadata.
//!
//! Called from:
//! - Arithmetic checking, EIR lowering and source validation.
//!
//! Key details:
//! - Trusted implementation statements retain their reviewed PHP arithmetic operations.
//! - User PHP and LFC operations inherit the same immutable compilation profile.

use crate::source::SourceMode;
use elephc_semantics::{arithmetic_policy::ArithmeticPolicy, SemanticProfile};
use elephc_semantics::{conversions::ValueKind, numeric_domains::{NumericDomainRule, NumericOperator}};
use crate::parser::ast::BinOp;
use crate::types::PhpType;

/// Resolves a source operation's policy without modifying the compilation's profile.
pub(crate) fn operation_policy(profile: SemanticProfile, source: SourceMode) -> ArithmeticPolicy {
    if source == SourceMode::Internal { ArithmeticPolicy::PERMISSIVE }
    else { ArithmeticPolicy::from_profile(profile) }
}

/// Maps existing AST operators to dependency-neutral numeric signatures.
pub(crate) fn operator(op: &BinOp) -> Option<NumericOperator> {
    Some(match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul => NumericOperator::Arithmetic,
        BinOp::Div => NumericOperator::Division, BinOp::Mod => NumericOperator::Modulo,
        BinOp::Pow => NumericOperator::Power,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => NumericOperator::Bitwise,
        BinOp::ShiftLeft | BinOp::ShiftRight => NumericOperator::Shift,
        _ => return None,
    })
}

/// Maps exact integer bit operations for shared source evidence and constant folding.
pub(crate) fn integer_bit_op(op: &BinOp) -> Option<elephc_semantics::numeric_domains::IntegerBitOp> {
    use elephc_semantics::numeric_domains::IntegerBitOp;
    Some(match op {
        BinOp::BitAnd => IntegerBitOp::And, BinOp::BitOr => IntegerBitOp::Or,
        BinOp::BitXor => IntegerBitOp::Xor, BinOp::ShiftLeft => IntegerBitOp::ShiftLeft,
        BinOp::ShiftRight => IntegerBitOp::ShiftRight, _ => return None,
    })
}

/// Resolves one user operator's domain while leaving trusted implementation arithmetic alone.
pub(crate) fn operand_rule(profile: SemanticProfile, source: SourceMode, op: &BinOp) -> Option<NumericDomainRule> {
    operator_rule(profile, source, operator(op)?)
}

/// Resolves an operator signature without changing the source's immutable profile.
pub(crate) fn operator_rule(profile: SemanticProfile, source: SourceMode, operator: NumericOperator) -> Option<NumericDomainRule> {
    if source == SourceMode::Internal { return None; }
    elephc_semantics::numeric_domains::operand_rule(profile, operator)
}

/// Maps concrete scalar checker types without conflating boolean and integer storage.
pub(crate) fn value_kind(ty: &PhpType) -> ValueKind {
    match ty {
        PhpType::Int => ValueKind::Integer, PhpType::Float => ValueKind::Float,
        PhpType::Bool | PhpType::False => ValueKind::Boolean, PhpType::Str => ValueKind::String,
        PhpType::Void => ValueKind::Null,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Never | PhpType::TaggedScalar => ValueKind::Dynamic,
        _ => ValueKind::NonScalar,
    }
}

/// Rejects known scalar-domain mismatches after array and native-object overload selection.
pub(crate) fn validate_binary_types(
    rule: NumericDomainRule, left: &PhpType, right: &PhpType, span: crate::span::Span,
) -> Result<(), crate::errors::CompileError> {
    let (left, right) = (value_kind(left), value_kind(right));
    // Dynamic alternatives may select an explicit native-class operator at execution time.
    if left == ValueKind::Dynamic || right == ValueKind::Dynamic { return Ok(()); }
    if rule.domain.allows(left) && rule.domain.allows(right) { return Ok(()); }
    Err(crate::errors::CompileError::new(span, &rule.message()))
}

/// Rejects a known unary operand kind after native object overload selection.
pub(crate) fn validate_unary_type(rule: NumericDomainRule, value: &PhpType, span: crate::span::Span)
    -> Result<(), crate::errors::CompileError> {
    let kind = value_kind(value);
    if kind == ValueKind::Dynamic || rule.domain.allows(kind) { return Ok(()); }
    Err(crate::errors::CompileError::new(span, &rule.message()))
}

/// Resolves only warning-free literal unary-plus values after source-domain validation.
/// Callers retain validation of the original kind; this helper does not authorize coercion.
pub(crate) fn literal_unary_plus(inner: &crate::parser::ast::Expr) -> Option<crate::parser::ast::Expr> {
    use crate::parser::ast::{Expr, ExprKind};
    let kind = match &inner.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) => inner.kind.clone(),
        ExprKind::BoolLiteral(value) => ExprKind::IntLiteral(i64::from(*value)),
        ExprKind::Null => ExprKind::IntLiteral(0),
        ExprKind::UnaryPlus(value) => return literal_unary_plus(value),
        ExprKind::StringLiteral(value) => {
            let scan = elephc_semantics::numeric_string::scan_numeric_prefix(value)?;
            if !scan.is_fully_numeric() { return None; }
            if !scan.is_float {
                if let Ok(value) = scan.text.parse::<i64>() { return Some(Expr::new(ExprKind::IntLiteral(value), inner.span)); }
            }
            ExprKind::FloatLiteral(scan.text.parse().ok()?)
        }
        _ => return None,
    };
    Some(Expr::new(kind, inner.span))
}
