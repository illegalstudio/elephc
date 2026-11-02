//! Purpose:
//! Rewrites builtin calls the backend lowers only with a literal argument, before the checker:
//! - `unpack($be ? 'N1' : 'V1', $bytes)` becomes `$be ? unpack('N1', $bytes) : unpack('V1', $bytes)`;
//! - `settype($v, $type)` becomes `__elephc_settype($v, $type)`, a built prelude function taking
//!   `mixed &$var`;
//! - `highlight_string()`/`highlight_file()` become the native-rendering helpers of
//!   `crate::highlight_prelude`, so highlighting links no interpreter.
//!
//! Called from:
//! - `crate::func_args::desugar`, which every front end runs after autoloaded files are spliced
//!   in and before the AST optimizer and the checker.
//!
//! Key details:
//! - The backend lowers `unpack()` only for a literal format, and refused Symfony's
//!   `MoFileLoader::readLong()`: one refused body costs the whole program a recompile round.
//!   Evaluation order and count stay php's: the condition runs first, then exactly ONE branch
//!   evaluates the remaining arguments once. Only `cond ? 'literal' : 'literal'` is split.
//! - `settype()` changes its variable's TYPE, which the checker never modelled for the builtin:
//!   `settype($param, 'bool')` left an untyped parameter typed as the string it received and read
//!   back `"1"`, and a run-time type (Nette's `Helpers::getCastStrategy()`) was refused. As a call
//!   to a user function with a by-reference `mixed` parameter, the existing by-reference machinery
//!   gives the caller's variable mixed storage, and the helper's `match` spells php's casts and
//!   its two `ValueError`s.

use crate::parser::ast::{CastType, Expr, ExprKind, Program, StmtKind};
use crate::parser::ast::visit::{walk_nodes_mut, NodeMut};
use crate::synthetic_class::{
    e_bool, e_call, e_cast, e_null, e_str, e_var, function, internal_declarations, s_assign,
    s_return, t_mixed,
};

/// Builtins whose first argument must be a literal format for the backend.
const LITERAL_FORMAT_BUILTINS: &[&str] = &["unpack"];

/// The prelude function every `settype()` call is routed to.
const SETTYPE_HELPER: &str = "__elephc_settype";

/// Rewrites every eligible call in `program`.
pub fn split_ternary_literal_formats(mut program: Program) -> Program {
    let mut uses_settype = false;
    let mut uses_highlight = false;
    walk_nodes_mut(&mut program, &mut |node| {
        if let NodeMut::Expr(expr) = node {
            if let Some(rewritten) = rewrite_highlight_call(expr, &mut uses_highlight)
                .or_else(|| rewrite_call(expr, &mut uses_settype))
            {
                *expr = rewritten;
            }
        }
    });
    if uses_highlight
        && !declares_function(&program, crate::highlight_prelude::HIGHLIGHT_STRING_HELPER)
    {
        let mut combined = crate::highlight_prelude::highlight_declarations();
        combined.extend(program);
        program = combined;
    }
    if uses_settype && !declares_function(&program, SETTYPE_HELPER) {
        let mut combined = settype_declarations();
        combined.extend(program);
        return combined;
    }
    program
}

/// Routes a direct `highlight_string()`/`highlight_file()` call to its native-rendering helper.
fn rewrite_highlight_call(expr: &Expr, uses_highlight: &mut bool) -> Option<Expr> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    let callee = name.last_segment().unwrap_or(name.as_str());
    let helper = crate::highlight_prelude::helper_for(callee)?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    *uses_highlight = true;
    Some(Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::from_parts(
                crate::names::NameKind::FullyQualified,
                vec![helper.to_string()],
            ),
            args: args.clone(),
        },
        expr.span,
    ))
}

/// Whether `program` already declares a top-level function named `name`.
fn declares_function(program: &Program, name: &str) -> bool {
    program.iter().any(|stmt| {
        matches!(&stmt.kind, StmtKind::FunctionDecl { name: declared, .. } if declared.eq_ignore_ascii_case(name))
    })
}

/// The rewritten form of one call, when it needs one.
fn rewrite_call(expr: &Expr, uses_settype: &mut bool) -> Option<Expr> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    let callee = name.last_segment().unwrap_or(name.as_str());
    if callee.eq_ignore_ascii_case("settype") && args.len() == 2 {
        *uses_settype = true;
        return Some(Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::from_parts(
                    crate::names::NameKind::FullyQualified,
                    vec![SETTYPE_HELPER.to_string()],
                ),
                args: args.clone(),
            },
            expr.span,
        ));
    }
    if !LITERAL_FORMAT_BUILTINS
        .iter()
        .any(|builtin| builtin.eq_ignore_ascii_case(callee))
    {
        return None;
    }
    let ExprKind::Ternary {
        condition,
        then_expr,
        else_expr,
    } = &args.first()?.kind
    else {
        return None;
    };
    if !matches!(then_expr.kind, ExprKind::StringLiteral(_))
        || !matches!(else_expr.kind, ExprKind::StringLiteral(_))
    {
        return None;
    }
    let branch = |format: &Expr| {
        let mut branch_args = args.clone();
        branch_args[0] = format.clone();
        Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: branch_args,
            },
            expr.span,
        )
    };
    Some(Expr::new(
        ExprKind::Ternary {
            condition: condition.clone(),
            then_expr: Box::new(branch(then_expr)),
            else_expr: Box::new(branch(else_expr)),
        },
        expr.span,
    ))
}

/// `function __elephc_settype(mixed &$var, string $type): bool` -- php's `settype()`:
/// the type name is case-insensitive, `resource` and unknown names raise `ValueError`.
fn settype_declarations() -> Program {
    internal_declarations(|| {
        let span = crate::span::Span::dummy();
        let throw_value_error = |message: &str| {
            Expr::new(
                ExprKind::Throw(Box::new(Expr::new(
                    ExprKind::NewObject {
                        class_name: crate::names::Name::from_parts(
                            crate::names::NameKind::FullyQualified,
                            vec!["ValueError".to_string()],
                        ),
                        args: vec![e_str(message)],
                    },
                    span,
                ))),
                span,
            )
        };
        let arm = |names: &[&str], value: Expr| (names.iter().map(|name| e_str(name)).collect(), value);
        let conversion = Expr::new(
            ExprKind::Match {
                subject: Box::new(e_call("strtolower", vec![e_var("type")])),
                arms: vec![
                    arm(&["bool", "boolean"], e_cast(CastType::Bool, e_var("var"))),
                    arm(&["int", "integer"], e_cast(CastType::Int, e_var("var"))),
                    arm(&["float", "double"], e_cast(CastType::Float, e_var("var"))),
                    arm(&["string"], e_cast(CastType::String, e_var("var"))),
                    arm(&["array"], e_cast(CastType::Array, e_var("var"))),
                    arm(&["object"], e_cast(CastType::Object, e_var("var"))),
                    arm(&["null"], e_null()),
                    arm(&["resource"], throw_value_error("Cannot convert to resource type")),
                ],
                default: Some(Box::new(throw_value_error(
                    "settype(): Argument #2 ($type) must be a valid type",
                ))),
            },
            span,
        );
        vec![function(SETTYPE_HELPER)
            .param_by_ref("var", Some(t_mixed()))
            .param("type", crate::parser::ast::TypeExpr::Str)
            .returns(crate::parser::ast::TypeExpr::Bool)
            .body(vec![s_assign("var", conversion), s_return(e_bool(true))])
            .build()]
    })
}
