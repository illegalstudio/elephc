//! Purpose:
//! Decides whether a parsed program references PHP's `similar_text()`, so the prelude is injected
//! only for programs that call it.
//!
//! Called from:
//! - `crate::similar_text_prelude::inject_if_used`.
//!
//! Key details:
//! - THE REFERENCE WALK IS `crate::prelude_prune::usage`, NOT A NEW ONE. Several preludes each
//!   carry a hand-written ~600-line exhaustive AST walk that differs from its neighbours only in
//!   the leaf predicate; `usage::collect` is the same walk, already shared, and already
//!   enumerating the channels a reference can arrive through. Another copy would be another thing
//!   to keep in step with the AST.
//! - LITERALS COUNT AS REFERENCES. `$f = 'similar_text'; $f($a, $b);` names the function without a
//!   call node, and so does `function_exists('similar_text')`; `usage` harvests string literals
//!   for precisely that reason. A false positive only injects declarations, which is harmless; a
//!   false negative would leave the builtin's lowering calling an undeclared function.
//! - There is no "does the program declare its own" half, unlike the `dir`/`gz` preludes:
//!   `similar_text` is a registry builtin that PHP refuses to redeclare, so no user definition can
//!   collide with the injected one.

use crate::parser::ast::Stmt;

/// The PHP function whose lowering needs this prelude.
///
/// Matched as a whole segment through `Usage::references`, which folds with `php_symbol_key` —
/// PHP function names are case-insensitive and may be written `\similar_text` or
/// `Some\similar_text`.
pub(super) const SIMILAR_TEXT_FUNCTIONS: &[&str] = &["similar_text"];

/// Returns whether the program references `similar_text`, so the prelude must be injected ahead
/// of user code.
pub(super) fn program_references_similar_text(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    SIMILAR_TEXT_FUNCTIONS
        .iter()
        .any(|name| usage.references(name))
        || SIMILAR_TEXT_FUNCTIONS
            .iter()
            .any(|name| usage.literals.contains(&crate::names::php_symbol_key(name)))
}
