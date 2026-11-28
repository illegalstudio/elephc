//! Purpose:
//! Rejects the expression forms PHP forbids in a constant expression, before a backed enum
//! case's value is bound and folded.
//!
//! Called from:
//! - `super::enums` while enum case metadata is collected.
//!
//! Key details:
//! - Folding binds class and global constants and prunes the branches they decide, so a
//!   forbidden form in a pruned branch (`C::YES ? 1 : $x`) would otherwise be accepted. The
//!   forms are checked first, pre-order and left to right like `zend_compile_const_expr`, with
//!   PHP's own messages.
//! - A branch is skipped only where PHP's compile-time evaluation already decides it: the
//!   deciding operand folds from literals and predefined (persistent) constants alone. User
//!   `const` declarations and class constants are not substituted at compile time by an
//!   opcache-enabled PHP (and not for a class declared later in the file without it), so a
//!   branch they decide is still checked.

use crate::errors::CompileError;
use crate::parser::ast::{
    ArrayEntry, BinOp, CallableTarget, CastType, Expr, ExprKind, StaticReceiver,
};
use crate::types::deferred_defaults::{literal_truthiness, prune_literal_branches};

use super::enum_backing::join_int_concats;

const INVALID: &str = "Constant expression contains invalid operations";

/// Returns PHP's compile error for the first forbidden form in constant expression `expr`.
pub(super) fn validate_constant_expression(expr: &Expr) -> Result<(), CompileError> {
    let fail = |message: &str| Err(CompileError::new(expr.span, message));
    match &expr.kind {
        ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::MagicConstant(_)
        | ExprKind::ConstRef(_) => Ok(()),
        ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Static, .. } => {
            fail("\"static::\" is not allowed in compile-time constants")
        }
        ExprKind::ClassConstant { receiver: StaticReceiver::Static } => {
            fail("static::class cannot be used for compile-time class name resolution")
        }
        ExprKind::ScopedConstantAccess { .. } | ExprKind::ClassConstant { .. } => Ok(()),
        ExprKind::BinaryOp { left, op: BinOp::NullCoalesce, right }
        | ExprKind::NullCoalesce { value: left, default: right } => {
            validate_constant_expression(left)?;
            match compile_time_value(left) {
                Some(value) if !matches!(value.kind, ExprKind::Null) => Ok(()),
                _ => validate_constant_expression(right),
            }
        }
        ExprKind::BinaryOp { left, op: op @ (BinOp::And | BinOp::Or), right } => {
            validate_constant_expression(left)?;
            let decided = compile_time_value(left).and_then(|value| literal_truthiness(&value))
                == Some(matches!(op, BinOp::Or));
            if decided { Ok(()) } else { validate_constant_expression(right) }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            validate_constant_expression(left)?;
            validate_constant_expression(right)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            validate_constant_expression(condition)?;
            match compile_time_value(condition).and_then(|value| literal_truthiness(&value)) {
                Some(true) => validate_constant_expression(then_expr),
                Some(false) => validate_constant_expression(else_expr),
                None => {
                    validate_constant_expression(then_expr)?;
                    validate_constant_expression(else_expr)
                }
            }
        }
        ExprKind::ShortTernary { value, default } => {
            validate_constant_expression(value)?;
            match compile_time_value(value).and_then(|value| literal_truthiness(&value)) {
                Some(true) => Ok(()),
                _ => validate_constant_expression(default),
            }
        }
        ExprKind::UnaryPlus(value) | ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Spread(value) => validate_constant_expression(value),
        ExprKind::Cast { target: CastType::Object, .. } => {
            fail("Object casts are not supported in this context")
        }
        ExprKind::Cast { expr: value, .. } => validate_constant_expression(value),
        ExprKind::ArrayAccess { array, index } => {
            validate_constant_expression(array)?;
            validate_constant_expression(index)
        }
        ExprKind::ArrayLiteral(items) => items.iter().try_for_each(validate_constant_expression),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().try_for_each(|(key, value)| {
            validate_constant_expression(key)?;
            validate_constant_expression(value)
        }),
        ExprKind::ArrayLiteralMixed(items) => items.iter().try_for_each(|item| match item {
            ArrayEntry::Spread(value) | ArrayEntry::Value(value) => {
                validate_constant_expression(value)
            }
            ArrayEntry::Keyed(key, value) => {
                validate_constant_expression(key)?;
                validate_constant_expression(value)
            }
            // `[&$x]` and `[k => &$x]` take a reference, which no constant expression can. The
            // parser currently rejects reference entries as unsupported before this runs.
            ArrayEntry::Reference(_) | ArrayEntry::KeyReference(..) => fail(INVALID),
        }),
        ExprKind::NewObject { .. } | ExprKind::NewGeneric { .. } | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. } | ExprKind::NewScopedObject { .. } => {
            fail("New expressions are not supported in this context")
        }
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            validate_constant_expression(object)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            validate_constant_expression(object)?;
            validate_constant_expression(property)
        }
        // PHP 8.5 constant expressions may hold static closures and first-class callables.
        ExprKind::Closure { is_arrow: false, is_static: true, .. } => Ok(()),
        ExprKind::Closure { is_arrow: false, is_static: false, .. } => {
            fail("Closures in constant expressions must be static")
        }
        ExprKind::FirstClassCallable(
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. },
        ) => Ok(()),
        _ => fail(INVALID),
    }
}

/// Folds an already validated operand the way PHP does at compile time: literals and
/// predefined constants only, operators and concatenations included. Returns `None` when the operand is not known until run time.
fn compile_time_value(expr: &Expr) -> Option<Expr> {
    let mut value = expr.clone();
    substitute_predefined_constants(&mut value);
    // PHP folds `'n' . 1` at compile time too; the shared folder joins only string operands.
    join_int_concats(&mut value);
    prune_literal_branches(&mut value);
    literal_truthiness(&value).map(|_| value)
}

/// Replaces predefined (persistent) constants with their literal values, children first, folding
/// each enclosing node so a decided branch disappears before its parent is folded.
fn substitute_predefined_constants(expr: &mut Expr) {
    if let ExprKind::ConstRef(name) = &expr.kind {
        if let Some(kind) = predefined_literal(name.as_str()) {
            expr.kind = kind;
        }
        return;
    }
    let visit = substitute_predefined_constants;
    match &mut expr.kind {
        ExprKind::BinaryOp { left, right, .. } => { visit(left); visit(right); }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            visit(condition); visit(then_expr); visit(else_expr);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            visit(value); visit(default);
        }
        ExprKind::UnaryPlus(value) | ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Spread(value) | ExprKind::Cast { expr: value, .. } => visit(value),
        ExprKind::ArrayAccess { array, index } => { visit(array); visit(index); }
        ExprKind::ArrayLiteral(items) => items.iter_mut().for_each(visit),
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items { visit(key); visit(value); }
        }
        ExprKind::ArrayLiteralMixed(items) => {
            for item in items { item.exprs_mut().for_each(visit); }
        }
        _ => return,
    }
    *expr = crate::optimize::fold_constant_expression(expr.clone());
    prune_literal_branches(expr);
}

/// The literal a predefined constant substitutes at compile time; stream constants (`STDIN`)
/// are resources, which PHP does not substitute.
fn predefined_literal(name: &str) -> Option<ExprKind> {
    let constant = elephc_builtin_contract::lookup_constant(name)?;
    if !matches!(constant.route, elephc_builtin_contract::ConstantRoute::Predefined)
        || matches!(constant.value, elephc_builtin_contract::ConstValue::StreamResource(_))
    {
        return None;
    }
    crate::types::predefined_constants::literal_of(constant.value)
}
