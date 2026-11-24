//! Purpose:
//! Converts source-level write operands to Magician's statement IR.
//!
//! Called from:
//! - AST-to-EvalIR lowering for runtime writes.
//!
//! Key details:
//! - Static and computed receivers retain the same interpreter write forms.

use elephc_chopper::syntax::parser::ast::{Expr, RuntimeWrite};

use crate::{
    errors::EvalParseError,
    eval_ir::{EvalBinOp, EvalConst, EvalExpr, EvalStmt},
};

use super::{Scope, lower_bin_op, lower_expression};

/// Lowers a source mutation without applying native assignment rewrites.
pub(super) fn lower_write(scope: &Scope<'_>, target: Expr, write: RuntimeWrite) -> Option<EvalStmt> {
    let target = lower_expression(scope, target)?;
    match write {
        RuntimeWrite::Assign { op, value } => {
            let op = match op {
                Some(op) => Some(lower_bin_op(op)?),
                None => None,
            };
            let value = lower_expression(scope, value)?;
            if let EvalExpr::ArrayGet { array, index } = &target {
                if let (EvalExpr::LoadVar(name), None) = (array.as_ref(), op) {
                    return Some(EvalStmt::ArraySetVar { name: name.clone(), index: *index.clone(), value });
                }
            }
            property_assignment_stmt(target, op, value).ok()
        }
        RuntimeWrite::Append(value) => {
            let value = lower_expression(scope, value)?;
            match target {
                EvalExpr::LoadVar(name) => Some(EvalStmt::ArrayAppendVar { name, value }),
                _ => property_array_append_stmt(target, value).ok(),
            }
        }
        RuntimeWrite::BindReference(source) => property_reference_bind_stmt(target, source).ok(),
        RuntimeWrite::IncDec { increment, .. } => property_inc_dec_stmt(target, increment).ok(),
    }
}

/// Converts a property mutation that yields the old or updated value.
pub(super) fn lower_write_expression(
    scope: &Scope<'_>,
    target: Expr,
    write: RuntimeWrite,
) -> Option<EvalExpr> {
    let (object, property, dynamic) = match lower_expression(scope, target)? {
        EvalExpr::PropertyGet { object, property } => {
            (object, Box::new(EvalExpr::Const(EvalConst::String(property))), false)
        }
        EvalExpr::DynamicPropertyGet { object, property } => (object, property, true),
        _ => return None,
    };
    Some(match write {
        RuntimeWrite::Assign { op: Some(op), value } => EvalExpr::PropertyCompoundAssign {
            object, property, dynamic, op: lower_bin_op(op)?,
            value: Box::new(lower_expression(scope, value)?),
        },
        RuntimeWrite::IncDec { increment, prefix } => EvalExpr::PropertyIncDec {
            object, property, dynamic, increment, prefix,
        },
        _ => return None,
    })
}

/// Lowers a simple or compound assignment to a member or a dimension.
pub(crate) fn property_assignment_stmt(
    target: EvalExpr,
    op: Option<EvalBinOp>,
    value: EvalExpr,
) -> Result<EvalStmt, EvalParseError> {
    let value = match op {
        Some(op) if matches!(target, EvalExpr::StaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyNameGet { .. }) => EvalExpr::Binary {
            op,
            left: Box::new(target.clone()),
            right: Box::new(value),
        },
        _ => value,
    };
    Ok(match (target, op) {
        (EvalExpr::ArrayGet { array, index }, op) => return property_array_set_stmt(*array, *index, op, value),
        (EvalExpr::PropertyGet { object, property }, None) => EvalStmt::PropertySet {
            object: *object, property, value,
        },
        (EvalExpr::PropertyGet { object, property }, Some(op)) => EvalStmt::PropertyCompoundAssign {
            object: *object, property, op, value,
        },
        (EvalExpr::DynamicPropertyGet { object, property }, None) => EvalStmt::DynamicPropertySet {
            object: *object, property: *property, value,
        },
        (EvalExpr::DynamicPropertyGet { object, property }, Some(op)) => EvalStmt::DynamicPropertyCompoundAssign {
            object: *object, property: *property, op, value,
        },
        (EvalExpr::StaticPropertyGet { class_name, property }, _) => EvalStmt::StaticPropertySet {
            class_name, property, value,
        },
        (EvalExpr::DynamicStaticPropertyGet { class_name, property }, _) => EvalStmt::DynamicStaticPropertySet {
            class_name: *class_name, property, value,
        },
        (EvalExpr::DynamicStaticPropertyNameGet { class_name, property }, _) => EvalStmt::DynamicStaticPropertyNameSet {
            class_name: *class_name, property: *property, value,
        },
        _ => return Err(EvalParseError::UnexpectedToken),
    })
}

/// Builds a property by-reference binding statement from a parsed property target.
pub(crate) fn property_reference_bind_stmt(
    target: EvalExpr,
    source: String,
) -> Result<EvalStmt, EvalParseError> {
    match target {
        EvalExpr::PropertyGet { object, property } => Ok(EvalStmt::PropertyReferenceBind {
            object: *object,
            property,
            source,
        }),
        EvalExpr::DynamicPropertyGet { object, property } => {
            Ok(EvalStmt::DynamicPropertyReferenceBind {
                object: *object,
                property: *property,
                source,
            })
        }
        EvalExpr::StaticPropertyGet { class_name, property } => Ok(EvalStmt::StaticPropertyReferenceBind {
            class_name, property, source,
        }),
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyReferenceBind {
            class_name: *class_name,
            property,
            source,
        }),
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyNameReferenceBind {
            class_name: *class_name,
            property: *property,
            source,
        }),
        _ => Err(EvalParseError::UnexpectedToken),
    }
}

/// Builds an object-property increment/decrement statement from a parsed property target.
pub(crate) fn property_inc_dec_stmt(target: EvalExpr, increment: bool) -> Result<EvalStmt, EvalParseError> {
    match target {
        EvalExpr::PropertyGet { object, property } => Ok(EvalStmt::PropertyIncDec {
            object: *object,
            property,
            increment,
        }),
        EvalExpr::DynamicPropertyGet { object, property } => Ok(EvalStmt::DynamicPropertyIncDec {
            object: *object,
            property: *property,
            increment,
        }),
        EvalExpr::StaticPropertyGet { class_name, property } => Ok(EvalStmt::StaticPropertyIncDec {
            class_name, property, increment,
        }),
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyIncDec {
            class_name: *class_name,
            property,
            increment,
        }),
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyNameIncDec {
            class_name: *class_name,
            property: *property,
            increment,
        }),
        _ => Err(EvalParseError::UnexpectedToken),
    }
}

/// Builds an object-property array append statement from a parsed property target.
pub(crate) fn property_array_append_stmt(target: EvalExpr, value: EvalExpr) -> Result<EvalStmt, EvalParseError> {
    match target {
        EvalExpr::PropertyGet { object, property } => Ok(EvalStmt::PropertyArrayAppend {
            object: *object,
            property,
            value,
        }),
        EvalExpr::DynamicPropertyGet { object, property } => {
            Ok(EvalStmt::DynamicPropertyArrayAppend {
                object: *object,
                property: *property,
                value,
            })
        }
        EvalExpr::StaticPropertyGet { class_name, property } => Ok(EvalStmt::StaticPropertyArrayAppend {
            class_name, property, value,
        }),
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyArrayAppend {
            class_name: *class_name,
            property,
            value,
        }),
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyNameArrayAppend {
            class_name: *class_name,
            property: *property,
            value,
        }),
        array => Ok(EvalStmt::DimensionWrite { array, index: None, op: None, value }),
    }
}

/// Builds an object-property array write statement from a parsed property target.
pub(crate) fn property_array_set_stmt(
    target: EvalExpr,
    index: EvalExpr,
    op: Option<EvalBinOp>,
    value: EvalExpr,
) -> Result<EvalStmt, EvalParseError> {
    match target {
        EvalExpr::PropertyGet { object, property } => Ok(EvalStmt::PropertyArraySet {
            object: *object,
            property,
            index,
            op,
            value,
        }),
        EvalExpr::DynamicPropertyGet { object, property } => Ok(EvalStmt::DynamicPropertyArraySet {
            object: *object,
            property: *property,
            index,
            op,
            value,
        }),
        EvalExpr::StaticPropertyGet { class_name, property } => Ok(EvalStmt::StaticPropertyArraySet {
            class_name, property, index, op, value,
        }),
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyArraySet {
            class_name: *class_name,
            property,
            index,
            op,
            value,
        }),
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => Ok(EvalStmt::DynamicStaticPropertyNameArraySet {
            class_name: *class_name,
            property: *property,
            index,
            op,
            value,
        }),
        array => Ok(EvalStmt::DimensionWrite { array, index: Some(index), op, value }),
    }
}
