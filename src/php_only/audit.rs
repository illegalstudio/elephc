//! Purpose:
//! Rejects non-PHP syntax while reusing the exhaustive shared AST visitor.
//!
//! Called from:
//! - Physical PHP source finalization and syntax-audit tests.
//!
//! Key details:
//! - Every violation is collected, including attributes and declaration defaults.

use crate::ast_visit::{AstVisitor, walk_program};
use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::Program;
use crate::span::Span;

/// Audits PHP syntax and collects every extension or reserved-name violation.
pub fn check(program: &Program) -> Vec<CompileError> {
    let mut audit = PhpOnlyAudit { errors: Vec::new() };
    walk_program(program, &mut audit);
    audit.errors
}

/// Maps syntax observations to the PHP-only diagnostic policy.
struct PhpOnlyAudit {
    errors: Vec<CompileError>,
}

impl AstVisitor for PhpOnlyAudit {
    /// Rejects a syntax extension reported by the shared traversal.
    fn extension(&mut self, span: Span, description: &str) {
        self.errors.push(CompileError::new(span,
            &format!("{description}; rejected by --php-only")));
    }

    /// Rejects compiler-reserved names only in the free-function namespace.
    fn function_name(&mut self, span: Span, name: &Name) {
        let bare = name.parts.last().map(String::as_str).unwrap_or_default();
        if bare.to_ascii_lowercase().starts_with("__elephc_") {
            self.extension(span, &format!("'{bare}' is reserved for the compiler ('__elephc_' prefix)"));
        }
    }
}
