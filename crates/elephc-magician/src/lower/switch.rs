//! Purpose:
//! Lowers Chopper switch cases in their original fallthrough order.
//!
//! Called from:
//! - Magician's shared AST-to-EvalIR lowering.
//!
//! Key details:
//! - The AST groups consecutive labels and keeps the default body in a separate field.

use elephc_chopper::syntax::parser::ast::{Expr, Stmt};

use crate::eval_ir::EvalSwitchCase;

use super::{Scope, lower_branch, lower_expression};

/// Restores individual case labels and puts the default back at its source position.
pub(super) fn lower_switch_cases(scope: &Scope<'_>, cases: Vec<(Vec<Expr>, Vec<Stmt>)>, default: Option<Vec<Stmt>>) -> Option<Vec<EvalSwitchCase>> {
    let mut default = match default {
        Some(body) => {
            let order = body.first().map_or(u32::MAX, |stmt| stmt.span.col);
            Some((order, EvalSwitchCase { condition: None, body: lower_branch(scope, body)? }))
        }
        None => None,
    };
    let mut arms = Vec::new();
    for (mut values, body) in cases {
        let first = values.first()?.span.col;
        if default.as_ref().is_some_and(|(order, _)| *order < first) {
            arms.push(default.take()?.1);
        }
        let last = values.pop()?;
        for value in values {
            arms.push(EvalSwitchCase {
                condition: Some(lower_expression(scope, value)?),
                body: Vec::new(),
            });
        }
        arms.push(EvalSwitchCase {
            condition: Some(lower_expression(scope, last)?),
            body: lower_branch(scope, body)?,
        });
    }
    if let Some((_, default)) = default {
        arms.push(default);
    }
    Some(arms)
}
