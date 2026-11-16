//! Purpose:
//! Resolves PHP relative names (`namespace\foo`) where the parser meets them, against the
//! namespace the parser is currently inside.
//!
//! Called from:
//! - `crate::parser::parse_with_recovery_inner()` (one scope per parsed file).
//! - `crate::parser::stmt` (statement nesting, namespace declarations and blocks), and the
//!   name, expression, statement, type and attribute parsers that accept a name.
//!
//! Key details:
//! - `namespace\foo` means "foo in the current namespace", so it is exactly `\Current\Ns\foo`,
//!   and plain `\foo` in the global namespace. Resolving it to that fully qualified `Name` at
//!   parse time keeps every later pass on the path it already has for `\Current\Ns\foo`.
//! - The namespace is tracked structurally by the namespace-statement parser rather than by a
//!   token scan: `namespace X;` sets it until the next declaration, and a braced
//!   `namespace X { ... }` sets it for its body and restores the previous one at its `}`, as the
//!   name resolver does. An enum case or method named `namespace` never reaches that parser.
//! - A namespace declaration is only accepted at the top level of a file, as in PHP, so the
//!   file-wide tracking here cannot disagree with the resolver, which scopes a declaration to
//!   the statement list it appears in.
//! - Only a `namespace` token directly followed by `\` is a relative prefix. A segment spelled
//!   `namespace` elsewhere in a name is not handled here.

use std::cell::{Cell, RefCell};

use crate::lexer::{SpannedToken, Token};

thread_local! {
    /// Segments of the namespace the parser is currently inside; empty for the global one.
    static CURRENT_NAMESPACE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// How many statements the parser is inside: 1 while it parses a top-level statement.
    static STATEMENT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Restores the enclosing parse's namespace and statement depth when a parse scope ends,
/// including when the parse unwinds from a panic, so no parse can leak its namespace into the
/// next one on the same thread.
struct ParseScope {
    previous_namespace: Vec<String>,
    previous_depth: usize,
}

impl Drop for ParseScope {
    /// Puts back the namespace and statement depth saved when the scope was entered.
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous_namespace);
        CURRENT_NAMESPACE.with(|current| *current.borrow_mut() = previous);
        STATEMENT_DEPTH.with(|depth| depth.set(self.previous_depth));
    }
}

/// Runs `parse` with the parser in the global namespace at the top level, restoring the
/// enclosing parse's state afterwards, whether `parse` returns or panics.
///
/// Every parsed file starts in the global namespace (an included file does not inherit its
/// includer's), and a nested parse must not leak its namespace into the parse that started it.
pub(super) fn with_global_namespace_scope<R>(parse: impl FnOnce() -> R) -> R {
    let _scope = ParseScope {
        previous_namespace: CURRENT_NAMESPACE
            .with(|current| std::mem::take(&mut *current.borrow_mut())),
        previous_depth: STATEMENT_DEPTH.with(|depth| depth.replace(0)),
    };
    parse()
}

/// Leaves one statement level when dropped; returned by [`enter_statement`].
pub(crate) struct StatementLevel;

impl Drop for StatementLevel {
    /// Steps the statement depth back out of the statement that was entered.
    fn drop(&mut self) {
        STATEMENT_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Enters one statement level for the statement the parser is about to parse; the level is
/// left when the returned guard is dropped, on every exit path.
pub(crate) fn enter_statement() -> StatementLevel {
    STATEMENT_DEPTH.with(|depth| depth.set(depth.get() + 1));
    StatementLevel
}

/// Returns whether the statement being parsed is a top-level statement of the file, not one
/// nested in a block, a function, a class or a braced namespace.
pub(crate) fn at_top_level_statement() -> bool {
    STATEMENT_DEPTH.with(|depth| depth.get() <= 1)
}

/// Makes `parts` the current namespace and returns the one it replaces, for a braced block to
/// hand back to [`restore_namespace`] at its closing brace.
pub(crate) fn enter_namespace(parts: Vec<String>) -> Vec<String> {
    CURRENT_NAMESPACE.with(|current| std::mem::replace(&mut *current.borrow_mut(), parts))
}

/// Restores the namespace a braced namespace block replaced.
pub(crate) fn restore_namespace(previous: Vec<String>) {
    CURRENT_NAMESPACE.with(|current| *current.borrow_mut() = previous);
}

/// Returns the segments of the namespace a relative name at this point resolves against.
pub(crate) fn current_namespace_parts() -> Vec<String> {
    CURRENT_NAMESPACE.with(|current| current.borrow().clone())
}

/// Returns true when the tokens at `pos` are the relative-name prefix `namespace\`.
pub(crate) fn relative_name_starts_at(tokens: &[SpannedToken], pos: usize) -> bool {
    matches!(tokens.get(pos), Some((Token::Namespace, _)))
        && matches!(tokens.get(pos + 1), Some((Token::Backslash, _)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a parse that panics inside its scope still restores the enclosing namespace
    /// and statement depth, so the namespace it had entered cannot leak into the next parse on
    /// the thread (the scope used to restore only when `parse` returned).
    #[test]
    fn scope_restores_namespace_and_depth_when_the_parse_panics() {
        let unwound = std::panic::catch_unwind(|| {
            with_global_namespace_scope(|| {
                enter_namespace(vec!["Leaked".to_string()]);
                std::mem::forget(enter_statement());
                panic!("parse failed");
            })
        });
        assert!(unwound.is_err());
        assert!(current_namespace_parts().is_empty());
        assert!(at_top_level_statement());
        assert_eq!(STATEMENT_DEPTH.with(Cell::get), 0);
    }
}
