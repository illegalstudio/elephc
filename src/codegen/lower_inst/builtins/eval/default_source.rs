//! Purpose:
//! Encodes normalized lazy default trees as compiler-generated expression source.
//!
//! Called from:
//! - Native eval default metadata registration for defaults containing deferred Errors.
//!
//! Key details:
//! - Magician parses this data once during registration, never calls a throwing native helper.
//! - Explicit parentheses preserve the compiler AST's grouping and source evaluation order.
//! - Names are fully qualified and strings are quoted without interpolation.

use super::*;
use crate::parser::ast::{ArrayEntry, BinOp, CastType};

/// Encodes one normalized default as a single return expression for owned EvalIR metadata.
pub(super) fn default_expression_source(
    expr: &Expr, context: &EvalNativeDefaultContext<'_>,
) -> Option<String> {
    let expression = expression(expr, context)?;
    Some(match context.current_class {
        Some(class) => format!("return __elephc_default_lexical_scope({}, {expression});", quoted(class)),
        None => format!("return {expression};"),
    })
}

/// Uses the implementation owner for inherited default visibility rather than the receiver class.
pub(super) fn declaring_default_context<'a>(
    module: &'a crate::ir::Module, class: &'a str, method: &str, is_static: bool,
) -> EvalNativeDefaultContext<'a> {
    let owner = module.class_infos.get(class).and_then(|info| {
        if is_static { info.static_method_impl_classes.get(method) }
        else { info.method_impl_classes.get(method) }
    }).map(String::as_str).unwrap_or(class);
    EvalNativeDefaultContext::for_class(module, owner)
}

/// Quotes a PHP string without changing backslashes, quotes, NULs or interpolation markers.
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Resolves a lexical class receiver before its default crosses into runtime eval metadata.
fn receiver_name(receiver: &StaticReceiver, context: &EvalNativeDefaultContext<'_>) -> Option<String> {
    const_default_values::resolve_static_receiver_name(
        &const_default_values::ConstDefaultContext {
            module: context.module, current_class: context.current_class,
        }, receiver,
    )
}

/// Preserves positional, named and unpacked source arguments in a default constructor.
fn arguments(args: &[Expr], context: &EvalNativeDefaultContext<'_>) -> Option<String> {
    args.iter().map(|arg| expression(arg, context)).collect::<Option<Vec<_>>>()
        .map(|args| args.join(", "))
}

/// Renders only the constant-expression grammar accepted for native parameter defaults.
fn expression(expr: &Expr, context: &EvalNativeDefaultContext<'_>) -> Option<String> {
    Some(match &expr.kind {
        ExprKind::Null => "null".to_string(),
        ExprKind::BoolLiteral(value) => value.to_string(),
        ExprKind::IntLiteral(value) => value.to_string(),
        ExprKind::FloatLiteral(value) if value.is_nan() => "\\NAN".to_string(),
        ExprKind::FloatLiteral(value) if value.is_infinite() =>
            if value.is_sign_negative() { "-\\INF" } else { "\\INF" }.to_string(),
        ExprKind::FloatLiteral(value) => format!("{value:?}"),
        ExprKind::StringLiteral(value) => quoted(value),
        ExprKind::ConstRef(name) => format!("\\{}", name.as_str().trim_start_matches('\\')),
        ExprKind::ClassConstant { receiver } => quoted(&receiver_name(receiver, context)?),
        ExprKind::ScopedConstantAccess { receiver, name } =>
            format!("\\{}::{name}", receiver_name(receiver, context)?),
        ExprKind::Throw(_) => {
            let message = crate::types::deferred_defaults::deferred_error_message(expr)?;
            format!("__elephc_deferred_default_error({})", quoted(message))
        }
        ExprKind::ArrayLiteral(values) => format!("[{}]", arguments(values, context)?),
        ExprKind::ArrayLiteralAssoc(entries) => {
            let values = entries.iter().map(|(key, value)| Some(format!("{} => {}",
                expression(key, context)?, expression(value, context)?)))
                .collect::<Option<Vec<_>>>()?;
            format!("[{}]", values.join(", "))
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            let values = entries.iter().map(|entry| Some(match entry {
                ArrayEntry::Value(value) => expression(value, context)?,
                ArrayEntry::Spread(value) => format!("...{}", expression(value, context)?),
                ArrayEntry::Keyed(key, value) => format!("{} => {}",
                    expression(key, context)?, expression(value, context)?),
            })).collect::<Option<Vec<_>>>()?;
            format!("[{}]", values.join(", "))
        }
        ExprKind::ArrayAccess { array, index } =>
            format!("({})[{}]", expression(array, context)?, expression(index, context)?),
        ExprKind::NewObject { class_name, args } => format!("new \\{}({})",
            class_name.as_str().trim_start_matches('\\'), arguments(args, context)?),
        ExprKind::NewScopedObject { receiver, args } => format!("new \\{}({})",
            receiver_name(receiver, context)?, arguments(args, context)?),
        ExprKind::NamedArg { name, value } => format!("{name}: {}", expression(value, context)?),
        ExprKind::Spread(value) => format!("...{}", expression(value, context)?),
        ExprKind::Negate(value) => format!("(-{})", expression(value, context)?),
        ExprKind::Not(value) => format!("(!{})", expression(value, context)?),
        ExprKind::BitNot(value) => format!("(~{})", expression(value, context)?),
        ExprKind::BinaryOp { left, op, right } => format!("({} {} {})",
            expression(left, context)?, binary_operator(op), expression(right, context)?),
        ExprKind::NullCoalesce { value, default } => format!("({} ?? {})",
            expression(value, context)?, expression(default, context)?),
        ExprKind::ShortTernary { value, default } => format!("({} ?: {})",
            expression(value, context)?, expression(default, context)?),
        ExprKind::Ternary { condition, then_expr, else_expr } => format!("({} ? {} : {})",
            expression(condition, context)?, expression(then_expr, context)?, expression(else_expr, context)?),
        ExprKind::Cast { target, expr } => {
            let target = match target {
                CastType::Int => "int", CastType::Float => "float", CastType::String => "string",
                CastType::Bool => "bool", CastType::Array => "array", CastType::Object => "object",
            };
            format!("(({target}) {})", expression(expr, context)?)
        }
        ExprKind::Match { subject, arms, default } => {
            let mut cases = arms.iter().map(|(conditions, value)| Some(format!("{} => {}",
                arguments(conditions, context)?, expression(value, context)?)))
                .collect::<Option<Vec<_>>>()?;
            if let Some(default) = default {
                cases.push(format!("default => {}", expression(default, context)?));
            }
            format!("match ({}) {{ {} }}", expression(subject, context)?, cases.join(", "))
        }
        _ => return None,
    })
}

/// Returns the PHP spelling of a constant-expression binary operator.
fn binary_operator(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
        BinOp::Mod => "%", BinOp::Concat => ".", BinOp::Eq => "==", BinOp::NotEq => "!=",
        BinOp::StrictEq => "===", BinOp::StrictNotEq => "!==", BinOp::Lt => "<", BinOp::Gt => ">",
        BinOp::LtEq => "<=", BinOp::GtEq => ">=", BinOp::Pow => "**", BinOp::And => "&&",
        BinOp::Or => "||", BinOp::Xor => "xor", BinOp::BitAnd => "&", BinOp::BitOr => "|",
        BinOp::BitXor => "^", BinOp::ShiftLeft => "<<", BinOp::ShiftRight => ">>",
        BinOp::Spaceship => "<=>", BinOp::NullCoalesce => "??",
    }
}
