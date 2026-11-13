//! Purpose:
//! Home of the PHP `parse_url` builtin and its refined result-type contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through the builtin registry.
//!
//! Key details:
//! - The runtime always returns a boxed Mixed cell, while the checker exposes PHP's
//!   array-or-false, component-or-null-or-false, and dynamic component shapes.
//! - Any negative static component selects the full array, matching PHP 8.4.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "parse_url",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ParseUrl,
    ),
}

/// Validates the optional component type and returns the corresponding PHP result union.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let component = match cx.args.get(1) {
        Some(component) => {
            let component_ty = cx.checker.infer_type(component, cx.env)?;
            if component_ty != PhpType::Int {
                return Err(CompileError::new(
                    component.span,
                    "parse_url() component must be int",
                ));
            }
            parse_url_static_component(component)
        }
        None => Some(-1),
    };
    match component {
        Some(component) if component < 0 => Ok(parse_url_array_result_type(cx)),
        Some(2) => Ok(cx.checker.normalize_union_type(vec![
            PhpType::Int,
            PhpType::Void,
            PhpType::False,
        ])),
        Some(0..=7) => Ok(cx.checker.normalize_union_type(vec![
            PhpType::Str,
            PhpType::Void,
            PhpType::False,
        ])),
        _ => Ok(PhpType::Mixed),
    }
}

/// Returns the heterogeneous associative-array-or-false type used by full parsing.
fn parse_url_array_result_type(cx: &BuiltinCheckCtx<'_>) -> PhpType {
    cx.checker.normalize_union_type(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        PhpType::False,
    ])
}

/// Resolves a static component expression, including folded `PHP_URL_*` combinations.
fn parse_url_static_component(component: &Expr) -> Option<i64> {
    match &component.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str().trim_start_matches('\\') {
            "PHP_URL_SCHEME" => Some(0),
            "PHP_URL_HOST" => Some(1),
            "PHP_URL_PORT" => Some(2),
            "PHP_URL_USER" => Some(3),
            "PHP_URL_PASS" => Some(4),
            "PHP_URL_PATH" => Some(5),
            "PHP_URL_QUERY" => Some(6),
            "PHP_URL_FRAGMENT" => Some(7),
            _ => None,
        },
        ExprKind::UnaryPlus(inner) => crate::semantics::numeric::literal_unary_plus(inner)
            .and_then(|number| parse_url_static_component(&number)),
        ExprKind::Negate(inner) => parse_url_static_component(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = parse_url_static_component(left)?;
            let right = parse_url_static_component(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
