//! Purpose:
//! Lowers PHP attributes to Magician's retained reflection metadata.
//!
//! Called from:
//! - Shared declaration and parameter lowering.
//!
//! Key details:
//! - Retains attribute names even when an argument cannot be materialized.

use elephc_chopper::syntax::parser::ast::AttributeGroup;

use crate::eval_ir::{EvalArrayElement, EvalAttribute, EvalAttributeArg, EvalConst, EvalExpr, EvalUnaryOp};

use super::{Scope, lower_args};

/// Converts attribute groups into one ordered declaration or parameter attribute list.
pub(super) fn lower_attributes(scope: &Scope<'_>, groups: Vec<AttributeGroup>) -> Vec<EvalAttribute> {
    groups.into_iter().flat_map(|group| group.attributes).map(|attribute| {
        let args = lower_args(scope, attribute.args).and_then(|args| {
            args.into_iter().map(|arg| {
                if arg.is_spread() {
                    return None;
                }
                let value = eval_attribute_arg_from_expr(arg.value())?;
                Some(match arg.name() {
                    Some(name) => EvalAttributeArg::Named { name: name.to_owned(), value: Box::new(value) },
                    None => value,
                })
            }).collect()
        });
        EvalAttribute::new(scope.class_name(&attribute.name), args)
    }).collect()
}

/// Converts a parsed attribute argument expression into retained literal metadata.
pub(crate) fn eval_attribute_arg_from_expr(expr: &EvalExpr) -> Option<EvalAttributeArg> {
    match expr {
        EvalExpr::Const(EvalConst::String(value)) => Some(EvalAttributeArg::String(value.clone())),
        EvalExpr::Const(EvalConst::Bytes(value)) => Some(EvalAttributeArg::Bytes(value.clone())),
        EvalExpr::Const(EvalConst::Int(value)) => Some(EvalAttributeArg::Int(*value)),
        EvalExpr::Const(EvalConst::Float(value)) => Some(EvalAttributeArg::Float(value.to_bits())),
        EvalExpr::Const(EvalConst::Bool(value)) => Some(EvalAttributeArg::Bool(*value)),
        EvalExpr::Const(EvalConst::Null) => Some(EvalAttributeArg::Null),
        EvalExpr::Unary {
            op: EvalUnaryOp::Negate,
            expr,
        } => match expr.as_ref() {
            EvalExpr::Const(EvalConst::Int(value)) => {
                Some(EvalAttributeArg::Int(value.wrapping_neg()))
            }
            EvalExpr::Const(EvalConst::Float(value)) => {
                Some(EvalAttributeArg::Float((-*value).to_bits()))
            }
            _ => None,
        },
        EvalExpr::ClassNameFetch { class_name } => {
            eval_attribute_class_name_arg(class_name).map(EvalAttributeArg::String)
        }
        EvalExpr::Array(elements) => eval_attribute_array_arg_from_elements(elements),
        _ => None,
    }
}

/// Converts an eval array literal into retained attribute metadata.
fn eval_attribute_array_arg_from_elements(
    elements: &[EvalArrayElement],
) -> Option<EvalAttributeArg> {
    elements
        .iter()
        .map(|element| match element {
            EvalArrayElement::Value(value) => eval_attribute_arg_from_expr(value),
            EvalArrayElement::Reference(_) => None,
            EvalArrayElement::KeyValue { key, value } => {
                let value = eval_attribute_arg_from_expr(value)?;
                eval_attribute_array_keyed_arg(key, value)
            }
            EvalArrayElement::KeyReference { .. } => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(EvalAttributeArg::Array)
}

/// Wraps an attribute array value with the PHP-normalized literal key metadata.
fn eval_attribute_array_keyed_arg(
    key: &EvalExpr,
    value: EvalAttributeArg,
) -> Option<EvalAttributeArg> {
    match key {
        EvalExpr::Const(EvalConst::String(name)) => Some(EvalAttributeArg::Named {
            name: name.clone(),
            value: Box::new(value),
        }),
        EvalExpr::Const(EvalConst::Int(key)) => Some(EvalAttributeArg::IntKeyed {
            key: *key,
            value: Box::new(value),
        }),
        EvalExpr::Const(EvalConst::Bool(key)) => Some(EvalAttributeArg::IntKeyed {
            key: i64::from(*key),
            value: Box::new(value),
        }),
        EvalExpr::Const(EvalConst::Null) => Some(EvalAttributeArg::Named {
            name: String::new(),
            value: Box::new(value),
        }),
        EvalExpr::Const(EvalConst::Float(key)) => Some(EvalAttributeArg::IntKeyed {
            key: *key as i64,
            value: Box::new(value),
        }),
        EvalExpr::Unary {
            op: EvalUnaryOp::Negate,
            expr,
        } => eval_attribute_array_negated_keyed_arg(expr, value),
        EvalExpr::ClassNameFetch { class_name } => {
            eval_attribute_class_name_arg(class_name).map(|name| EvalAttributeArg::Named {
                name,
                value: Box::new(value),
            })
        }
        _ => None,
    }
}

/// Wraps an attribute array value with a normalized negative numeric literal key.
fn eval_attribute_array_negated_keyed_arg(
    key: &EvalExpr,
    value: EvalAttributeArg,
) -> Option<EvalAttributeArg> {
    match key {
        EvalExpr::Const(EvalConst::Int(key)) => Some(EvalAttributeArg::IntKeyed {
            key: key.wrapping_neg(),
            value: Box::new(value),
        }),
        EvalExpr::Const(EvalConst::Float(key)) => Some(EvalAttributeArg::IntKeyed {
            key: (-*key) as i64,
            value: Box::new(value),
        }),
        _ => None,
    }
}

/// Returns a compile-time class-name string for named `ClassName::class` attribute args.
fn eval_attribute_class_name_arg(class_name: &str) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    if ["self", "parent", "static"]
        .iter()
        .any(|special| class_name.eq_ignore_ascii_case(special))
    {
        return None;
    }
    Some(class_name.to_string())
}
