//! Purpose:
//! Routes PHP 8.6's `fopen()` and `stream_context_set_default()` through their error-aware wrappers.
//!
//! Called from:
//! - `crate::pfa::desugar_for_version` for every compilation profile at or above PHP 8.6.
//!
//! Key details:
//! - The wrappers are ordinary PHP functions declared by `crate::stream_errors_prelude`; the
//!   rewrite happens after name resolution and before the checker, so calls resolve normally.
//! - The wrappers' own bodies keep calling the original builtins; `wrapper_depth` prevents the
//!   rewrite from turning those into recursion.
//! - When a program does not reach `fopen` or `stream_context_set_default`, their wrappers are
//!   absent from the declaration list and the rewrite is a no-op.

use crate::magic_constants::walker::Pass;
use crate::names::{Name, NameKind, php_symbol_key};
use crate::parser::ast::{Expr, ExprKind, MagicConstant, Program, Stmt, StmtKind};
use crate::span::Span;

/// Rewrites the PHP 8.6 stream calls that have an error-aware wrapper in `program`.
pub(crate) fn rewrite(program: Program, version: crate::php_version::PhpVersion) -> Program {
    if version < crate::php_version::PhpVersion::Php86 {
        return program;
    }
    let wrappers = wrappers_present(&program);
    if !wrappers.fopen && !wrappers.default_context {
        return program;
    }
    let mut pass = Rewriter {
        wrappers,
        wrapper_depth: 0,
    };
    crate::magic_constants::walker::walk_program(program, &mut pass)
}

/// Which wrapper functions the injected prelude declared.
#[derive(Clone, Copy, Default)]
struct Wrappers {
    fopen: bool,
    default_context: bool,
}

/// Scans the top-level declarations for the two wrapper functions.
fn wrappers_present(program: &[Stmt]) -> Wrappers {
    let mut wrappers = Wrappers::default();
    for stmt in program {
        if let StmtKind::FunctionDecl { name, .. } = &stmt.kind {
            match php_symbol_key(name).as_str() {
                "__elephc_fopen" => wrappers.fopen = true,
                "__elephc_stream_context_set_default" => wrappers.default_context = true,
                _ => {}
            }
        }
    }
    wrappers
}

/// Rewrites calls while tracking whether the walk is inside a wrapper body.
struct Rewriter {
    wrappers: Wrappers,
    wrapper_depth: usize,
}

impl Pass for Rewriter {
    /// Leaves magic constants unchanged; they are already lowered before this pass.
    fn transform_magic(&self, _span: Span, magic: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(magic)
    }

    /// Rewrites one call after its arguments have been walked.
    fn transform_expr(&self, expr: Expr) -> Expr {
        let ExprKind::FunctionCall { name, args } = expr.kind else {
            return expr;
        };
        if self.wrapper_depth > 0 {
            return Expr::new(ExprKind::FunctionCall { name, args }, expr.span);
        }
        let key = php_symbol_key(name.as_str());
        let replacement = match key.as_str() {
            "fopen" if self.wrappers.fopen => Some("__elephc_fopen"),
            "stream_context_set_default" if self.wrappers.default_context => {
                Some("__elephc_stream_context_set_default")
            }
            _ => None,
        };
        let name = replacement.map_or(name, |replacement| {
            Name::from_parts(NameKind::FullyQualified, vec![replacement.to_string()])
        });
        Expr::new(ExprKind::FunctionCall { name, args }, expr.span)
    }

    /// Enters a function body, noticing the wrapper declarations.
    fn enter_function(&mut self, name: &str) {
        if matches!(
            php_symbol_key(name).as_str(),
            "__elephc_fopen" | "__elephc_stream_context_set_default"
        ) {
            self.wrapper_depth += 1;
        }
    }

    /// Leaves a function body.
    fn leave_function(&mut self) {
        self.wrapper_depth = self.wrapper_depth.saturating_sub(1);
    }
}
