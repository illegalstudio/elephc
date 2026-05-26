//! Purpose:
//! Rewrites file-dependent magic constants into string literals for a single source file.
//! Handles `__FILE__` and `__DIR__` before includes are merged into the main AST.
//!
//! Called from:
//! - `crate::magic_constants::substitute_file_constants()`.
//!
//! Key details:
//! - File paths are captured from the source file being walked, not from the including file.

use std::path::Path;

use crate::parser::ast::{ExprKind, MagicConstant, Stmt};
use crate::span::Span;

use super::walker::{walk_program, Pass};

/// Rewrites `__FILE__` and `__DIR__` magic constants into their string literal equivalents for a single source file.
///
/// Inputs:
/// - `stmts`: The AST statements to transform
/// - `file_path`: The absolute path to the source file being processed
///
/// Output: New `Vec<Stmt>` with all `__FILE__` replaced by the file's canonical path string and all `__DIR__` replaced by its directory.
///
/// Note: The file path is canonicalized before conversion to a string to resolve symlinks and relative paths.
pub(super) fn substitute_file_constants(stmts: Vec<Stmt>, file_path: &Path) -> Vec<Stmt> {
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let file = canonical.display().to_string();
    let dir = canonical
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut pass = FilePass { file, dir };
    walk_program(stmts, &mut pass)
}

/// Holds the resolved file path and directory strings used to substitute `__FILE__` and `__DIR__` magic constants.
struct FilePass {
    file: String,
    dir: String,
}

impl Pass for FilePass {
    /// Transforms `__FILE__` → the stored file path string and `__DIR__` → the stored directory string.
    /// All other `MagicConstant` variants are returned unchanged.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        match mc {
            MagicConstant::File => ExprKind::StringLiteral(self.file.clone()),
            MagicConstant::Dir => ExprKind::StringLiteral(self.dir.clone()),
            other => ExprKind::MagicConstant(other),
        }
    }
}
