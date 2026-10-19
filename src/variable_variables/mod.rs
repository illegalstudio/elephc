//! Purpose:
//! Desugars PHP variable variables (`$$name`, `${expr}`) and the literal-name scope builtins
//! `compact()` / `extract()` into plain PHP over the frame's compile-time locals, so the
//! optimizer, the checker and EIR lowering only ever see ordinary variable reads and writes.
//!
//! Called from:
//! - `crate::pipeline::compile()` after `func_args::desugar()` and before the AST optimizer.
//! - `crate::parser::variable_variables` builds the frame-access shape through [`frame_access`].
//!
//! Key details:
//! - AOT model: every local a PHP frame can hold under a name the source spells out is known at
//!   compile time. A variable variable is lowered to a dispatch on its runtime name string over
//!   that set (the scope's *frame names*). Because the dispatch is ordinary PHP, every later pass
//!   sees the real reads and writes: a dynamic write that changes a local's type widens it to
//!   mixed through the checker's normal branch merge, constant propagation is invalidated by the
//!   explicit assignments, and EIR lowering boxes/unboxes each native slot as usual.
//! - Reads (`$$n`, `$$n[k]`, `$$n->p`, `$$n->m()`) become
//!   `match ((string) n) { 'a' => $a ?? null, ..., default => <Undefined variable warning, null> }`.
//!   The `??` probe keeps a frame name that is declared but not yet assigned from being a
//!   compile-time `Undefined variable` error; it reads as `null` (PHP also warns there).
//! - Writes (`$$n = v`, compound assignments, `$$n[k] = v`, `$$n->p = v`, `$$n++`), `unset($$n)`,
//!   `foreach ($$n as &$v)` and `$r = &$$n` are statement-level: the name is evaluated once into a
//!   string temporary and an `if`/`elseif` chain writes the matching local. A name the frame does
//!   not know would create a new local in PHP; that throws an explicit `Error` instead.
//! - Write candidates are narrowed when the name is statically bounded (literal, `foreach` over a
//!   literal array, `switch` case labels, string-valued definitions). Narrowing never decides
//!   correctness: a runtime name outside the narrowed set reaches the `Error` branch, so an
//!   imprecise bound can only fail loudly, never write the wrong slot.
//! - Top-level code is rejected for non-literal names: main's locals are typed from the final
//!   global environment, where a probe of a later-assigned name is not representable.
//! - `${'name'}` with a literal valid name is exactly `$name` and is substituted directly.

mod builtins;
mod respan;
mod rewrite;
mod scan;
mod scope;
pub(crate) mod static_props;
mod walk;

use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program};
use crate::span::Span;

/// Unforgeable pseudo-variable naming the current frame's variable table in parser output.
///
/// `#` cannot appear in a PHP variable name, so no source program can spell it, and the `#gen`
/// tag keeps it out of every PHP-visible local enumeration should it ever survive.
pub const FRAME_LOCAL: &str = "__elephc_varvar_frame#gen";

/// PHP superglobals, which variable variables cannot reach inside functions or methods.
const SUPERGLOBALS: [&str; 9] = [
    "GLOBALS", "_SERVER", "_GET", "_POST", "_FILES", "_COOKIE", "_SESSION", "_REQUEST", "_ENV",
];

/// Builds the parser's frame-access shape for a variable variable whose name is `name`.
pub fn frame_access(name: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(FRAME_LOCAL.to_string()), span)),
            index: Box::new(name),
        },
        span,
    )
}

/// Returns the name expression when `expr` is a frame access (`$$name` / `${expr}`).
pub(crate) fn frame_access_name(expr: &Expr) -> Option<&Expr> {
    match &expr.kind {
        ExprKind::ArrayAccess { array, index } => match &array.kind {
            ExprKind::Variable(name) if name == FRAME_LOCAL => Some(index),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the frame access at the root of a place chain (`$$n`, `$$n[k]`, `$$n->p[k]`, ...).
pub(crate) fn place_frame_root(expr: &Expr) -> Option<&Expr> {
    if frame_access_name(expr).is_some() {
        return Some(expr);
    }
    match &expr.kind {
        ExprKind::ArrayAccess { array: inner, .. }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::DynamicPropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object: inner, .. } => place_frame_root(inner),
        _ => None,
    }
}

/// Clones a place chain, replacing its root frame access with `replacement`.
pub(crate) fn replace_place_root(expr: &Expr, replacement: &Expr) -> Expr {
    if frame_access_name(expr).is_some() {
        return replacement.clone();
    }
    let kind = match &expr.kind {
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(replace_place_root(array, replacement)),
            index: index.clone(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(replace_place_root(object, replacement)),
            property: property.clone(),
        },
        ExprKind::DynamicPropertyAccess { object, property } => ExprKind::DynamicPropertyAccess {
            object: Box::new(replace_place_root(object, replacement)),
            property: property.clone(),
        },
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(replace_place_root(object, replacement)),
                property: property.clone(),
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(replace_place_root(object, replacement)),
                property: property.clone(),
            }
        }
        other => other.clone(),
    };
    Expr::new(kind, expr.span)
}

/// Returns whether `name` is a PHP variable a variable variable can reach in a function frame.
pub(crate) fn is_frame_reachable_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('#')
        && name != "this"
        && !SUPERGLOBALS.contains(&name)
}

/// Returns whether a literal variable-variable name is a plain PHP identifier.
pub(crate) fn is_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || !first.is_ascii())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii())
}

/// Desugars every variable variable and literal-name scope builtin in `program`.
pub fn desugar(program: Program) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        let mut program = program;
        let (static_box, static_errors) = static_props::desugar(&mut program);
        if !static_errors.is_empty() {
            return Err(CompileError::from_many(static_errors));
        }
        let mut rewriter = rewrite::Rewriter::new();
        let rewritten = rewriter.run_top_level(&mut program);
        if rewriter.used_box || static_box {
            program.insert(0, box_helper_decl());
        }
        let mut errors = rewriter.into_errors();
        if rewritten && errors.is_empty() {
            for span in scan::leftover_frame_accesses(&mut program) {
                errors.push(CompileError::new(
                    span,
                    "Unsupported variable-variable form (elephc): this position cannot take `$$name` or `${expr}`",
                ));
            }
        }
        match errors {
            errors if errors.is_empty() => Ok(program),
            errors => Err(CompileError::from_many(errors)),
        }
    })
}

/// Name of the injected identity helper whose `mixed` signature boxes a dispatch arm.
///
/// A `match` whose arms read locals of different native types must produce one boxed value.
/// Routing every arm through `fn(mixed $v): mixed` gives each arm the same static type, so the
/// merge never has to reconcile heterogeneous native representations.
pub const BOX_HELPER: &str = "__elephc_varvar_mixed";

/// Builds `function __elephc_varvar_mixed(mixed $v): mixed { return $v; }`.
fn box_helper_decl() -> crate::parser::ast::Stmt {
    use crate::parser::ast::{Stmt, StmtKind, TypeExpr};
    let span = Span::dummy();
    let mixed = || TypeExpr::Named(Name::unqualified("mixed"));
    let mut decl = Stmt::new(
        StmtKind::FunctionDecl {
            name: BOX_HELPER.to_string(),
            type_params: Vec::new(),
            params: vec![("v".to_string(), Some(mixed()), None, false)],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: Some(mixed()),
            by_ref_return: false,
            body: vec![Stmt::new(StmtKind::Return(Some(var("v", span))), span)],
        },
        span,
    );
    decl.source_mode = crate::source::SourceMode::Internal;
    if let StmtKind::FunctionDecl { body, .. } = &mut decl.kind {
        for stmt in body {
            stmt.source_mode = crate::source::SourceMode::Internal;
        }
    }
    decl
}

/// Wraps `value` in a call to the mixed-boxing helper.
pub(crate) fn box_mixed(value: Expr) -> Expr {
    let span = value.span;
    global_call(BOX_HELPER, vec![value], span)
}

// --- small AST builders shared by the rewrite modules ---

pub(crate) fn var(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

pub(crate) fn str_lit(value: &str, span: Span) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), span)
}

pub(crate) fn cast_string(expr: Expr) -> Expr {
    let span = expr.span;
    Expr::new(
        ExprKind::Cast {
            target: CastType::String,
            expr: Box::new(expr),
        },
        span,
    )
}

pub(crate) fn binop(left: Expr, op: BinOp, right: Expr) -> Expr {
    let span = left.span;
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        },
        span,
    )
}

pub(crate) fn global_call(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec![name.to_string()]),
            args,
        },
        span,
    )
}

/// Builds `new \Error(<message>)`.
pub(crate) fn new_error(message: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::NewObject {
            class_name: Name::from_parts(NameKind::FullyQualified, vec!["Error".to_string()]),
            args: vec![message],
        },
        span,
    )
}

/// Builds `throw new \Error(<message>)` as an expression.
pub(crate) fn throw_error(message: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::Throw(Box::new(Expr::new(
            ExprKind::NewObject {
                class_name: Name::from_parts(NameKind::FullyQualified, vec!["Error".to_string()]),
                args: vec![message],
            },
            span,
        ))),
        span,
    )
}

/// Builds `trigger_error(<message>, E_USER_WARNING) ? null : null`: PHP's warning, then `null`.
pub(crate) fn warning_then_null(message: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(global_call(
                "trigger_error",
                vec![message, Expr::new(ExprKind::IntLiteral(512), span)],
                span,
            )),
            then_expr: Box::new(Expr::new(ExprKind::Null, span)),
            else_expr: Box::new(Expr::new(ExprKind::Null, span)),
        },
        span,
    )
}

/// Returns whether evaluating `expr` twice is indistinguishable from evaluating it once.
pub(crate) fn is_pure_name_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name != FRAME_LOCAL,
        ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::This => true,
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => is_pure_name_expr(left) && is_pure_name_expr(right),
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => is_pure_name_expr(expr),
        _ => false,
    }
}
