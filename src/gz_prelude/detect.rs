//! Purpose:
//! Decides whether a parsed program references PHP's `gz*` stream surface, so the prelude is
//! injected only for programs that read or write a gzip stream — and whether the program already
//! declares one of those names itself, so a user definition is never clobbered with a
//! redeclaration error.
//!
//! Called from:
//! - `crate::gz_prelude::inject_if_used`.
//!
//! Key details:
//! - THE REFERENCE WALK IS `crate::prelude_prune::usage`, NOT A NEW ONE. Nine preludes each carry
//!   a hand-written ~600-line exhaustive AST walk that differs from its neighbours only in the
//!   leaf predicate; `usage::collect` is the same walk, already shared, already used by
//!   `web_prelude` for exactly this pay-for-use question, and already enumerating the channels a
//!   reference can arrive through (imports, callback strings, `function_exists` probes). A tenth
//!   copy would be a tenth thing to keep in step with the AST.
//! - LITERALS COUNT AS REFERENCES. `$f = 'gzread'; $f($h, 8);` names the function without a call
//!   node, and `usage` harvests every string literal for precisely that reason. A false positive
//!   only injects declarations, which is harmless; a false negative would turn a valid program
//!   into "Call to undefined function".
//! - The names are matched through `Usage::references`, which folds with `php_symbol_key` — PHP
//!   function names are case-insensitive and may be written `\gzopen` or `Some\gzopen`, and the
//!   walk records the normalized form.
//! - `readgzfile` and `gzputs` are in the set: they are php's own names for two of these, and a
//!   program may reference only one of them.

use crate::parser::ast::{Stmt, StmtKind};

/// The PHP functions this prelude declares. A program naming ANY of them gets all of them —
/// they are one surface, and splitting the injection would make the cheap case (one function)
/// pay for a reachability decision that the declaration-pruning pass already makes later.
pub(super) const GZ_FUNCTIONS: &[&str] = &[
    "gzopen",
    "gzclose",
    "gzeof",
    "gzgetc",
    "gzgets",
    "gzread",
    "gzwrite",
    "gzputs",
    "gzpassthru",
    "gzrewind",
    "gzseek",
    "gztell",
    "gzfile",
    "readgzfile",
    // The string half of the surface: these frame BYTES rather than serve a stream, but they live
    // in the same prelude and a program naming one needs it injected just the same.
    "gzencode",
    "gzdecode",
    "zlib_encode",
    "zlib_decode",
    "zlib_get_coding_type",
];

/// Returns whether the program references any `gz*` stream function, so the prelude must be
/// injected ahead of user code.
pub(super) fn program_uses_gz(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    GZ_FUNCTIONS.iter().any(|name| usage.references(name))
        || GZ_FUNCTIONS
            .iter()
            .any(|name| usage.literals.contains(&crate::names::php_symbol_key(name)))
}

/// Returns whether the program already declares one of these functions itself, in which case the
/// prelude must not be injected so the user definition wins.
///
/// A polyfill is the reason this exists: a program targeting builds without ext-zlib may well
/// carry `if (!function_exists('gzopen')) { function gzopen(...) {...} }`, and injecting on top of
/// that would turn a working program into "Cannot redeclare".
pub(super) fn program_declares_gz(program: &[Stmt]) -> bool {
    program.iter().any(stmt_declares_gz)
}

/// Returns whether a statement declares one of these functions, recursing only into the block
/// forms that can host a hoisted declaration.
fn stmt_declares_gz(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => GZ_FUNCTIONS
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known)),
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body.iter().any(stmt_declares_gz),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            // `if (!function_exists('gzopen')) { function gzopen(…) {…} }` is the polyfill shape,
            // and its declaration lives inside the branch rather than at the top level.
            then_body.iter().any(stmt_declares_gz)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body.iter().any(stmt_declares_gz))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_declares_gz))
        }
        _ => false,
    }
}
