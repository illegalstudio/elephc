use std::path::Path;

use crate::parser::ast::{ExprKind, MagicConstant, Stmt};
use crate::span::Span;

use super::walker::{walk_program, Pass};

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

struct FilePass {
    file: String,
    dir: String,
}

impl Pass for FilePass {
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        match mc {
            MagicConstant::File => ExprKind::StringLiteral(self.file.clone()),
            MagicConstant::Dir => ExprKind::StringLiteral(self.dir.clone()),
            other => ExprKind::MagicConstant(other),
        }
    }
}
