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
    if !wrappers.fopen
        && !wrappers.default_context
        && !wrappers.fread
        && !wrappers.fwrite
        && !wrappers.file_get_contents
        && !wrappers.stream_get_contents
        && !wrappers.stream_select
        && !wrappers.context_create
        && !wrappers.context_get_options
        && !wrappers.context_set_option
    {
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
    fread: bool,
    fwrite: bool,
    file_get_contents: bool,
    stream_get_contents: bool,
    stream_select: bool,
    context_create: bool,
    context_get_options: bool,
    context_set_option: bool,
}

/// Scans the top-level declarations for the two wrapper functions.
fn wrappers_present(program: &[Stmt]) -> Wrappers {
    let mut wrappers = Wrappers::default();
    for stmt in program {
        if let StmtKind::FunctionDecl { name, .. } = &stmt.kind {
            match php_symbol_key(name).as_str() {
                "__elephc_fopen" => wrappers.fopen = true,
                "__elephc_stream_context_set_default" => wrappers.default_context = true,
                "__elephc_fread" => wrappers.fread = true,
                "__elephc_fwrite" => wrappers.fwrite = true,
                "__elephc_file_get_contents" => wrappers.file_get_contents = true,
                "__elephc_stream_get_contents" => wrappers.stream_get_contents = true,
                "__elephc_stream_select" => wrappers.stream_select = true,
                "__elephc_stream_context_create" => wrappers.context_create = true,
                "__elephc_stream_context_get_options" => wrappers.context_get_options = true,
                "__elephc_stream_context_set_option" => wrappers.context_set_option = true,
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
            "fread" if self.wrappers.fread => Some("__elephc_fread"),
            "fwrite" if self.wrappers.fwrite => Some("__elephc_fwrite"),
            "file_get_contents" if self.wrappers.file_get_contents => {
                Some("__elephc_file_get_contents")
            }
            "stream_get_contents" if self.wrappers.stream_get_contents => {
                Some("__elephc_stream_get_contents")
            }
            "stream_select" if self.wrappers.stream_select => Some("__elephc_stream_select"),
            "stream_context_create" if self.wrappers.context_create => {
                Some("__elephc_stream_context_create")
            }
            "stream_context_get_options" if self.wrappers.context_get_options => {
                Some("__elephc_stream_context_get_options")
            }
            "stream_context_set_option" if self.wrappers.context_set_option => {
                Some("__elephc_stream_context_set_option")
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
            "__elephc_fopen"
                | "__elephc_stream_context_set_default"
                | "__elephc_fread"
                | "__elephc_fwrite"
                | "__elephc_file_get_contents"
                | "__elephc_file_get_contents_fallback"
                | "__elephc_stream_get_contents"
                | "__elephc_stream_select"
                | "__elephc_stream_context_create"
                | "__elephc_stream_context_get_options"
                | "__elephc_stream_context_set_option"
                | "__elephc_stream_failure"
                | "__elephc_stream_apply_errors"
                | "__elephc_stream_read_flush"
                | "__elephc_stream_select_flush"
        ) {
            self.wrapper_depth += 1;
        }
    }

    /// Leaves a function body.
    fn leave_function(&mut self) {
        self.wrapper_depth = self.wrapper_depth.saturating_sub(1);
    }
}
