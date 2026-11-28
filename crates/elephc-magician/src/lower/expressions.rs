//! Purpose:
//! Lowers Chopper expressions to Magician's evaluation forms.
//!
//! Called from:
//! - Shared statement, declaration and member lowering.
//!
//! Key details:
//! - Runtime operands, names and call argument order are retained.

use elephc_chopper::syntax::{
    names::NameKind,
    parser::ast::{
        ArrayEntry, BinOp, CallableTarget, CastType, Expr, ExprKind, InstanceOfTarget,
        MagicConstant, DYNAMIC_CALLABLE_FACTORY,
    },
};

use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalCastType, EvalConst, EvalExpr,
    EvalInstanceOfTarget, EvalMagicConst, EvalMatchArm, EvalUnaryOp,
};

use super::{Scope, classes, closures, is_unset_construct, local_name, members, ordinary_class_name};

/// Lowers scalar expressions while leaving their values and arithmetic to the interpreter.
pub(super) fn lower_expression(scope: &Scope<'_>, expr: Expr) -> Option<EvalExpr> {
    Some(match expr.kind {
        ExprKind::Runtime(expression) => members::lower_runtime_expression(scope, *expression)?,
        ExprKind::Closure { .. } => closures::lower_closure(scope, expr)?,
        ExprKind::IntLiteral(value) => EvalExpr::Const(EvalConst::Int(value)),
        ExprKind::FloatLiteral(value) => EvalExpr::Const(EvalConst::Float(value)),
        ExprKind::StringLiteral(value) => EvalExpr::Const(
            match String::from_utf8(elephc_builtin_contract::string_literal::literal_bytes(&value)) {
                Ok(value) => EvalConst::String(value),
                Err(error) => EvalConst::Bytes(error.into_bytes()),
            }
        ),
        ExprKind::BoolLiteral(value) => EvalExpr::Const(EvalConst::Bool(value)),
        ExprKind::Null => EvalExpr::Const(EvalConst::Null),
        ExprKind::MagicConstant(magic) => EvalExpr::Magic(match magic {
            MagicConstant::Line => EvalMagicConst::Line(i64::from(expr.span.line)),
            MagicConstant::File => EvalMagicConst::File,
            MagicConstant::Dir => EvalMagicConst::Dir,
            MagicConstant::Function => EvalMagicConst::Function,
            MagicConstant::Class => EvalMagicConst::Class,
            MagicConstant::Method => EvalMagicConst::Method,
            MagicConstant::Trait => EvalMagicConst::Trait,
            MagicConstant::Namespace => return Some(EvalExpr::Const(EvalConst::String(scope.namespace.clone()))),
        }),
        ExprKind::Variable(name) => EvalExpr::LoadVar(local_name(name)?),
        ExprKind::This => EvalExpr::LoadVar("this".to_owned()),
        ExprKind::Print(value) => EvalExpr::Print(Box::new(lower_expression(scope, *value)?)),
        ExprKind::Clone(value) => EvalExpr::Clone(Box::new(lower_expression(scope, *value)?)),
        ExprKind::InstanceOf { value, target } => {
            let target = match target {
                InstanceOfTarget::Name(name) if name.kind == NameKind::Unqualified
                    && ["self", "parent", "static"].iter().any(|word| name.as_str().eq_ignore_ascii_case(word)) => {
                    EvalInstanceOfTarget::ClassName(name.as_str().to_ascii_lowercase())
                }
                InstanceOfTarget::Name(name) => EvalInstanceOfTarget::ClassName(ordinary_class_name(scope, name)?),
                InstanceOfTarget::Expr(value) => EvalInstanceOfTarget::Expr(Box::new(lower_expression(scope, *value)?)),
                InstanceOfTarget::Generic(ty) => EvalInstanceOfTarget::ClassName(
                    super::templates::lower_structural_type(scope, ty)?.to_string(),
                ),
            };
            EvalExpr::InstanceOf { value: Box::new(lower_expression(scope, *value)?), target }
        }
        ExprKind::Cast { target, expr } => EvalExpr::Cast {
            target: match target {
                CastType::Array => EvalCastType::Array,
                CastType::Int => EvalCastType::Int,
                CastType::Float => EvalCastType::Float,
                CastType::String => EvalCastType::String,
                CastType::StringFormat => EvalCastType::StringFormat,
                CastType::Bool => EvalCastType::Bool,
                CastType::Object => return None,
            },
            expr: Box::new(lower_expression(scope, *expr)?),
        },
        ExprKind::IncludeValue { path, once, required } => EvalExpr::Include {
            path: Box::new(lower_expression(scope, *path)?), once, required,
        },
        ExprKind::ConstRef(name) => match scope.constant_name(&name) {
            (name, Some(fallback_name)) => EvalExpr::NamespacedConstFetch { name, fallback_name },
            (name, None) => EvalExpr::ConstFetch(name),
        },
        ExprKind::FunctionCall { name, args } if !is_unset_construct(&name) => {
            let args = lower_args(scope, args)?;
            match scope.function_name(&name) {
                (name, Some(fallback_name)) => EvalExpr::NamespacedCall { name, fallback_name, args },
                (name, None) => EvalExpr::Call { name, args },
            }
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) if !is_unset_construct(&name) => {
            let (name, fallback_name) = scope.function_name(&name);
            EvalExpr::FunctionCallable { name, fallback_name }
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => EvalExpr::StaticMethodCallable {
            class_name: classes::lower_receiver(scope, receiver)?,
            method: Box::new(EvalExpr::Const(EvalConst::String(method))),
        },
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => EvalExpr::MethodCallable {
            object: Box::new(lower_expression(scope, *object)?),
            method: Box::new(EvalExpr::Const(EvalConst::String(method))),
        },
        ExprKind::ClassConstant { receiver } => EvalExpr::ClassNameFetch {
            class_name: classes::lower_receiver(scope, receiver)?,
        },
        ExprKind::ScopedConstantAccess { receiver, name } => EvalExpr::ClassConstantFetch {
            class_name: classes::lower_receiver(scope, receiver)?,
            constant: name,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => EvalExpr::StaticPropertyGet {
            class_name: classes::lower_receiver(scope, receiver)?,
            property,
        },
        ExprKind::StaticMethodCall { method, args, .. } if method == DYNAMIC_CALLABLE_FACTORY => {
            let callee = lower_expression(scope, args.into_iter().next()?)?;
            EvalExpr::InvokableCallable { object: Box::new(callee) }
        }
        ExprKind::StaticMethodCall { receiver, method, args } => EvalExpr::StaticMethodCall {
            class_name: classes::lower_receiver(scope, receiver)?,
            method,
            args: lower_args(scope, args)?,
        },
        ExprKind::NewObject { class_name, args }
            if elephc_chopper::syntax::parser::is_anonymous_class_name(class_name.as_str()) =>
        {
            let args = lower_args(scope, args)?;
            let declaration = scope.anonymous_classes.get(class_name.as_str())?.clone();
            EvalExpr::NewAnonymousClass { class: classes::lower_class(scope, declaration)?, args }
        }
        ExprKind::NewObject { class_name, args } => EvalExpr::NewObject {
            class_name: ordinary_class_name(scope, class_name)?,
            args: lower_args(scope, args)?,
        },
        ExprKind::NewScopedObject { receiver, args } => EvalExpr::NewObject {
            class_name: classes::lower_receiver(scope, receiver)?,
            args: lower_args(scope, args)?,
        },
        ExprKind::NewGeneric { class_type, args } => EvalExpr::NewObject {
            class_name: super::templates::lower_structural_type(scope, class_type)?.to_string(),
            args: lower_args(scope, args)?,
        },
        ExprKind::NewDynamic { name_expr, args } => EvalExpr::DynamicNewObject {
            class_name: Box::new(lower_expression(scope, *name_expr)?),
            args: lower_args(scope, args)?,
        },
        ExprKind::PropertyAccess { object, property } => EvalExpr::PropertyGet {
            object: Box::new(lower_expression(scope, *object)?),
            property,
        },
        ExprKind::NullsafePropertyAccess { object, property } => EvalExpr::NullsafePropertyGet {
            object: Box::new(lower_expression(scope, *object)?),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => EvalExpr::DynamicPropertyGet {
            object: Box::new(lower_expression(scope, *object)?),
            property: Box::new(lower_expression(scope, *property)?),
        },
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => EvalExpr::NullsafeDynamicPropertyGet {
            object: Box::new(lower_expression(scope, *object)?),
            property: Box::new(lower_expression(scope, *property)?),
        },
        ExprKind::MethodCall { object, method, args } => EvalExpr::MethodCall {
            object: Box::new(lower_expression(scope, *object)?),
            method,
            args: lower_args(scope, args)?,
        },
        ExprKind::NullsafeMethodCall { object, method, args } => EvalExpr::NullsafeMethodCall {
            object: Box::new(lower_expression(scope, *object)?),
            method,
            args: lower_args(scope, args)?,
        },
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => EvalExpr::NullsafeDynamicMethodCall {
            object: Box::new(lower_expression(scope, *object)?),
            method: Box::new(lower_expression(scope, *method)?),
            args: lower_args(scope, args)?,
        },
        ExprKind::ClosureCall { var, args } => EvalExpr::DynamicCall {
            callee: Box::new(EvalExpr::LoadVar(local_name(var)?)),
            args: lower_args(scope, args)?,
        },
        ExprKind::ExprCall { callee, args } => EvalExpr::DynamicCall {
            callee: Box::new(lower_expression(scope, *callee)?),
            args: lower_args(scope, args)?,
        },
        ExprKind::ArrayLiteral(elements) => EvalExpr::Array(
            elements
                .into_iter()
                .map(|value| lower_expression(scope, value).map(EvalArrayElement::Value))
                .collect::<Option<_>>()?,
        ),
        ExprKind::ArrayLiteralMixed(elements) => EvalExpr::Array(
            elements
                .into_iter()
                .map(|entry| match entry {
                    ArrayEntry::Value(value) => lower_expression(scope, value).map(EvalArrayElement::Value),
                    ArrayEntry::Keyed(key, value) => Some(EvalArrayElement::KeyValue {
                        key: lower_expression(scope, key)?,
                        value: lower_expression(scope, value)?,
                    }),
                    ArrayEntry::Reference(value) => lower_expression(scope, value).map(EvalArrayElement::Reference),
                    ArrayEntry::KeyReference(key, value) => Some(EvalArrayElement::KeyReference {
                        key: lower_expression(scope, key)?,
                        value: lower_expression(scope, value)?,
                    }),
                    ArrayEntry::Spread(_) => None,
                })
                .collect::<Option<_>>()?,
        ),
        ExprKind::ArrayAccess { array, index } => EvalExpr::ArrayGet {
            array: Box::new(lower_expression(scope, *array)?),
            index: Box::new(lower_expression(scope, *index)?),
        },
        ExprKind::Assignment { target, value, result_target: None, prelude, conditional_value_temp: None }
            if prelude.is_empty() => {
                let ExprKind::Variable(name) = target.kind else { return None };
                EvalExpr::Assign { name: local_name(name)?, value: Box::new(lower_expression(scope, *value)?) }
            }
        ExprKind::NullCoalesce { value, default } => EvalExpr::NullCoalesce {
            value: Box::new(lower_expression(scope, *value)?),
            default: Box::new(lower_expression(scope, *default)?),
        },
        ExprKind::Match { subject, arms, default } => EvalExpr::Match {
            subject: Box::new(lower_expression(scope, *subject)?),
            arms: arms.into_iter().map(|(patterns, value)| {
                Some(EvalMatchArm {
                    patterns: patterns.into_iter().map(|value| lower_expression(scope, value)).collect::<Option<_>>()?,
                    value: lower_expression(scope, value)?,
                })
            }).collect::<Option<_>>()?,
            default: match default {
                Some(value) => Some(Box::new(lower_expression(scope, *value)?)),
                None => None,
            },
        },
        ExprKind::Ternary { condition, then_expr, else_expr } => EvalExpr::Ternary {
            condition: Box::new(lower_expression(scope, *condition)?),
            then_branch: Some(Box::new(lower_expression(scope, *then_expr)?)),
            else_branch: Box::new(lower_expression(scope, *else_expr)?),
        },
        ExprKind::ShortTernary { value, default } => EvalExpr::Ternary {
            condition: Box::new(lower_expression(scope, *value)?),
            then_branch: None,
            else_branch: Box::new(lower_expression(scope, *default)?),
        },
        ExprKind::ErrorSuppress(inner) => EvalExpr::ErrorSuppress(Box::new(lower_expression(scope, *inner)?)),
        ExprKind::Throw(inner) => EvalExpr::Throw(Box::new(lower_expression(scope, *inner)?)),
        ExprKind::Negate(inner) => lower_unary(scope, *inner, EvalUnaryOp::Negate)?,
        ExprKind::Not(inner) => lower_unary(scope, *inner, EvalUnaryOp::LogicalNot)?,
        ExprKind::BitNot(inner) => lower_unary(scope, *inner, EvalUnaryOp::BitNot)?,
        ExprKind::BinaryOp { left, op, right } => {
            let op = lower_bin_op(op)?;
            EvalExpr::Binary {
                op,
                left: Box::new(lower_expression(scope, *left)?),
                right: Box::new(lower_expression(scope, *right)?),
            }
        }
        _ => return None,
    })
}

/// Maps shared arithmetic and comparison operators to the interpreter.
pub(super) fn lower_bin_op(op: BinOp) -> Option<EvalBinOp> {
    Some(match op {
        BinOp::Add => EvalBinOp::Add,
        BinOp::Sub => EvalBinOp::Sub,
        BinOp::Mul => EvalBinOp::Mul,
        BinOp::Div => EvalBinOp::Div,
        BinOp::Mod => EvalBinOp::Mod,
        BinOp::Concat => EvalBinOp::Concat,
        BinOp::Pow => EvalBinOp::Pow,
        BinOp::And => EvalBinOp::LogicalAnd,
        BinOp::Or => EvalBinOp::LogicalOr,
        BinOp::Xor => EvalBinOp::LogicalXor,
        BinOp::BitAnd => EvalBinOp::BitAnd,
        BinOp::BitOr => EvalBinOp::BitOr,
        BinOp::BitXor => EvalBinOp::BitXor,
        BinOp::ShiftLeft => EvalBinOp::ShiftLeft,
        BinOp::ShiftRight => EvalBinOp::ShiftRight,
        BinOp::Eq => EvalBinOp::LooseEq,
        BinOp::NotEq => EvalBinOp::LooseNotEq,
        BinOp::StrictEq => EvalBinOp::StrictEq,
        BinOp::StrictNotEq => EvalBinOp::StrictNotEq,
        BinOp::Lt => EvalBinOp::Lt,
        BinOp::LtEq => EvalBinOp::LtEq,
        BinOp::Gt => EvalBinOp::Gt,
        BinOp::GtEq => EvalBinOp::GtEq,
        BinOp::Spaceship => EvalBinOp::Spaceship,
        _ => return None,
    })
}

/// Lowers call arguments in written order, preserving names and unpacking.
pub(super) fn lower_args(scope: &Scope<'_>, args: Vec<Expr>) -> Option<Vec<EvalCallArg>> {
    args.into_iter().map(|arg| match arg.kind {
        ExprKind::Spread(value) => lower_expression(scope, *value).map(EvalCallArg::spread),
        ExprKind::NamedArg { name, value } => Some(EvalCallArg::named(name, lower_expression(scope, *value)?)),
        _ => lower_expression(scope, arg).map(EvalCallArg::positional),
    }).collect()
}

/// Preserves the interpreter's own coercion rules for a unary operation.
pub(super) fn lower_unary(scope: &Scope<'_>, expr: Expr, op: EvalUnaryOp) -> Option<EvalExpr> {
    Some(EvalExpr::Unary { op, expr: Box::new(lower_expression(scope, expr)?) })
}
