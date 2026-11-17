//! Purpose:
//! Reuses include-loaded function bodies when Composer also names their physical file eagerly.
//!
//! Called from:
//! - `autoload::run_collecting_included_on_compiler_stack`, before eager files join the program.
//!
//! Key details:
//! - A physical declaration is identified by its canonical path and source coordinates.
//! - Eager execution activates the existing include variant before the file's effects run.
//! - Declarations from different files remain distinct and retain redeclaration diagnostics.

use std::path::{Path, PathBuf};

use crate::fast_hash::FastMap as HashMap;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

/// One physical declaration, independent of the source identity assigned by each parse.
type Origin = (PathBuf, u32, u32);

/// Include variants already declared by the entry's resolved include graph.
pub(super) struct ExistingVariants {
    by_declaration: HashMap<(String, Origin), (String, String)>,
}

impl ExistingVariants {
    /// Indexes public include dispatchers by the exact physical declaration of each variant.
    pub(super) fn from_program(program: &[Stmt]) -> Self {
        let definitions: HashMap<_, _> = program.iter().filter_map(|stmt| {
            let StmtKind::FunctionDecl { name, .. } = &stmt.kind else {
                return None;
            };
            physical_origin(stmt.span).map(|origin| (php_symbol_key(name), origin))
        }).collect();
        let mut by_declaration = HashMap::default();
        for stmt in program {
            let StmtKind::FunctionVariantGroup { name, variants } = &stmt.kind else {
                continue;
            };
            for variant in variants {
                if let Some(origin) = definitions.get(&php_symbol_key(variant)) {
                    by_declaration.insert(
                        (php_symbol_key(name), origin.clone()),
                        (name.clone(), variant.clone()),
                    );
                }
            }
        }
        Self { by_declaration }
    }

    /// Replaces duplicate eager declarations with activation of their existing include bodies.
    ///
    /// Marks precede all file effects, including calls written before a function declaration.
    /// Returning the reused private names keeps backend deferral attributed to the eager file.
    pub(super) fn reuse(&self, loaded: Program, path: &Path) -> (Program, Vec<String>) {
        let span = loaded.first().map_or_else(Span::dummy, |stmt| stmt.span);
        let mut marks = Vec::new();
        let mut body = Vec::with_capacity(loaded.len());
        let mut declarations = Vec::new();
        let mut reused = Vec::new();
        for stmt in loaded {
            let binding = match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => physical_origin(stmt.span)
                    .and_then(|origin| self.by_declaration.get(&(php_symbol_key(name), origin))),
                _ => None,
            };
            if let Some((name, variant)) = binding {
                marks.push(Stmt::new(
                    StmtKind::FunctionVariantMark { name: name.clone(), variant: variant.clone() },
                    stmt.span,
                ));
                reused.push(php_symbol_key(variant));
            } else if matches!(
                stmt.kind,
                StmtKind::FunctionDecl { .. }
                    | StmtKind::FunctionVariantGroup { .. }
                    | StmtKind::ClassDecl { .. }
                    | StmtKind::InterfaceDecl { .. }
                    | StmtKind::TraitDecl { .. }
                    | StmtKind::EnumDecl { .. }
            ) {
                declarations.push(stmt);
            } else {
                body.push(stmt);
            }
        }
        marks.extend(body);
        // The entry's existing once include must see this eager execution as its first load.
        // Keep declarations global while guarding effects and variant activation together.
        declarations.push(Stmt::new(
            StmtKind::IncludeOnceGuard {
                source_path: path.to_path_buf(),
                required: true,
                body: marks,
            },
            span,
        ));
        (declarations, reused)
    }
}

/// Resolves the physical location shared by separately parsed copies of one declaration.
fn physical_origin(span: Span) -> Option<Origin> {
    let path = PathBuf::from(span.source_path()?);
    let canonical = crate::source::canonical(&path).unwrap_or(path);
    Some((canonical, span.line, span.col))
}
