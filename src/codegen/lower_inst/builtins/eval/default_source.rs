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
//! - The smallest signed integer uses exact arithmetic to avoid an overflowing positive token.

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

/// Quotes literal PHP bytes separately from ordinary UTF-8 scope names and diagnostics.
fn quoted_literal(value: &str) -> String {
    let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
    if let Ok(text) = std::str::from_utf8(&bytes) {
        return quoted(text);
    }
    let escaped = bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    format!("\"{escaped}\"")
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
        ExprKind::IntLiteral(value) if *value == i64::MIN =>
            "(-9223372036854775807 - 1)".to_string(),
        ExprKind::IntLiteral(value) => value.to_string(),
        ExprKind::FloatLiteral(value) if value.is_nan() => "\\NAN".to_string(),
        ExprKind::FloatLiteral(value) if value.is_infinite() =>
            if value.is_sign_negative() { "-\\INF" } else { "\\INF" }.to_string(),
        ExprKind::FloatLiteral(value) => format!("{value:?}"),
        ExprKind::StringLiteral(value) => quoted_literal(value),
        ExprKind::ConstRef(name) => format!("\\{}", name.as_str().trim_start_matches('\\')),
        ExprKind::ClassConstant { receiver } => quoted(&receiver_name(receiver, context)?),
        ExprKind::ScopedConstantAccess { receiver, name } =>
            format!("\\{}::{name}", receiver_name(receiver, context)?),
        ExprKind::Throw(_) => {
            let message = crate::types::deferred_defaults::deferred_error_message(expr)?;
            format!("__elephc_deferred_default_error({})", quoted(message))
        }
        ExprKind::ArrayLiteral(values) => {
            let values = values.iter().map(|value| match &value.kind {
                ExprKind::Spread(value) => Some(format!("__elephc_default_array_spread({})", expression(value, context)?)),
                _ => expression(value, context),
            }).collect::<Option<Vec<_>>>()?;
            format!("[{}]", values.join(", "))
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            let values = entries.iter().map(|(key, value)| Some(format!("{} => {}",
                expression(key, context)?, expression(value, context)?)))
                .collect::<Option<Vec<_>>>()?;
            format!("[{}]", values.join(", "))
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            let values = entries.iter().map(|entry| Some(match entry {
                ArrayEntry::Value(value) => expression(value, context)?,
                ArrayEntry::Spread(value) => {
                    // Mixed literals retain the parser's Spread wrapper inside the entry too.
                    let value = match &value.kind { ExprKind::Spread(value) => value.as_ref(), _ => value };
                    format!("__elephc_default_array_spread({})", expression(value, context)?)
                }
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

/// Integer-min source uses only representable integer tokens when Magician parses it.
#[cfg(test)]
#[test]
fn native_default_minimum_integer_source_round_trips() {
    let module = Module::new(crate::codegen::platform::Target::parse("linux-x86_64").unwrap());
    let context = EvalNativeDefaultContext::for_class(&module, "Defaults");
    let expr = Expr::new(ExprKind::IntLiteral(i64::MIN), crate::span::Span::dummy());
    let source = default_expression_source(&expr, &context).unwrap();
    assert_eq!(source,
        "return __elephc_default_lexical_scope('Defaults', (-9223372036854775807 - 1));");
    elephc_magician::parser::parse_fragment(source.as_bytes()).expect("exact integer expression");
}

/// The encoder and decoder agree on nested spread selectors with explicit keys.
#[cfg(test)]
#[test]
fn native_default_spread_selector_source_round_trips() {
    let tokens = crate::lexer::tokenize(r#"<?php
class SelectedSpread {
    const ENABLED = true;
    public static function value(array $items = [10 => 'ten', ...(self::ENABLED ? [4 => 'four', 'name' => 'old'] : [parent::A]), 'name' => 'new', 'tail']): array { return $items; }
}
"#).unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let checked = crate::types::check(&program).unwrap();
    let module = Module::new(crate::codegen::platform::Target::parse("linux-x86_64").unwrap());
    let context = EvalNativeDefaultContext::for_class(&module, "SelectedSpread");
    let default = checked.classes["SelectedSpread"].static_methods["value"].defaults[0].as_ref().unwrap();
    let source = default_expression_source(default, &context).unwrap();
    let parsed = elephc_magician::parser::parse_fragment(source.as_bytes())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert!(!source.contains("__elephc_default_array_spread(..."), "{source}: {parsed:?}");
}
