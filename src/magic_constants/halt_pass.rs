//! Purpose:
//! Lowers `__COMPILER_HALT_OFFSET__` references using physical-file metadata.
//! Keeps offsets file-local before resolver include/autoload programs are flattened.
//!
//! Called from:
//! - `crate::source::finalize_physical_program()` after extracting the parser sentinel.
//!
//! Key details:
//! - Direct references and literal `constant()` calls become byte-exact integer literals.
//! - Namespace-relative and differently cased names remain ordinary PHP constants.

use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, MagicConstant, Program, Stmt, StmtKind};
use crate::span::Span;
use std::path::Path;

use super::walker::{walk_program, Pass};

const HALT_OFFSET_NAME: &str = "__COMPILER_HALT_OFFSET__";

/// Rewrites special halt-offset expression forms to the physical source byte offset.
pub(super) fn substitute_halt_compiler_constants(
    program: Program,
    offset: i64,
    file_path: &Path,
) -> Program {
    let mut program = walk_program(program, &mut HaltPass { offset });
    program.insert(0, mangled_offset_declaration(file_path, offset));
    program
}

/// Builds PHP's hidden `\0__COMPILER_HALT_OFFSET__\0<filename>` declaration.
fn mangled_offset_declaration(file_path: &Path, offset: i64) -> Stmt {
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let name = format!(
        "{}{}",
        crate::source::HALT_OFFSET_MANGLED_PREFIX,
        canonical.display()
    );
    Stmt::new(
        StmtKind::ConstDecl {
            name,
            value: Expr::int_lit(offset),
        },
        Span::dummy(),
    )
}

/// File-local expression pass carrying one parsed physical source offset.
struct HaltPass {
    offset: i64,
}

impl Pass for HaltPass {
    /// Leaves ordinary magic constants for the established file/scope passes.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Replaces PHP's direct constant and literal dynamic-fetch forms.
    fn transform_expr(&self, _span: Span, kind: ExprKind) -> ExprKind {
        match kind {
            ExprKind::ConstRef(name) if is_direct_halt_offset_name(&name) => {
                ExprKind::IntLiteral(self.offset)
            }
            ExprKind::FunctionCall { name, args }
                if is_global_constant_call(&name)
                    && args.len() == 1
                    && is_halt_offset_constant_argument(&args[0].kind) =>
            {
                ExprKind::IntLiteral(self.offset)
            }
            other => other,
        }
    }
}

/// Returns whether a source constant name receives PHP's halt-offset special handling.
fn is_direct_halt_offset_name(name: &Name) -> bool {
    matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified)
        && name.parts.len() == 1
        && name.parts[0] == HALT_OFFSET_NAME
}

/// Returns whether a call targets the global `constant()` builtin surface.
fn is_global_constant_call(name: &Name) -> bool {
    matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified)
        && name.parts.len() == 1
        && name.parts[0].eq_ignore_ascii_case("constant")
}

/// Recognizes positional and PHP-named literal arguments to `constant(string $name)`.
fn is_halt_offset_constant_argument(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::StringLiteral(value) => {
            value.strip_prefix('\\').unwrap_or(value) == HALT_OFFSET_NAME
        }
        ExprKind::NamedArg { name, value } if name == "name" => {
            is_halt_offset_constant_argument(&value.kind)
        }
        _ => false,
    }
}
