//! Purpose:
//! The canonical set of HTTP-request superglobals exposed under `--web`, the shared
//! PhpType for them, and the pass that creates the ones PHP's CLI SAPI would already
//! have created. Single source of truth consumed by the type checker, the IR lowering
//! global-storage path, and `__rt_web_reset`.
//!
//! Called from:
//! - `crate::types::checker` (seeding), `crate::ir_lower::context` (global
//!   storage), `crate::codegen::web` (per-request reset), `crate::optimize`
//!   (`seed_cli_populated_superglobals`), `crate::pipeline` (the `--web` flag).
//!
//! Key details:
//! - These names use `_eir_global_*` symbol storage in EVERY scope (true
//!   superglobals), unlike `$argc`/`$argv` which are top-level only.
//! - `SUPERGLOBALS` (what `--web` exposes) and `CLI_POPULATED_SUPERGLOBALS` (what a
//!   CLI program finds already there) are DIFFERENT sets, both measured against
//!   reference PHP rather than derived from each other.

use crate::types::PhpType;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether this compile targets `--web`, for the passes that cannot be handed the
/// flag through their signature.
///
/// `seed_cli_populated_superglobals` runs from `optimize::fold_constants`, and that
/// has THIRTEEN call sites: the driver plus twelve test harnesses that each rebuild
/// the phase order by hand (`tests/codegen/support/compiler.rs` says so in a
/// comment: "Mirrors `pipeline::compile`"). A pass that changes what a program
/// MEANS must not be one of the things those harnesses can forget to mirror, or the
/// suites keep pinning the behaviour the driver no longer has. Riding along inside a
/// call they all already make is what makes it unforgettable; this flag is the price.
/// Same shape as `codegen::set_null_repr`. Default `false`: a plain CLI compile.
static COMPILING_FOR_WEB: AtomicBool = AtomicBool::new(false);

/// Records whether the compile in progress is a `--web` build.
pub fn set_compiling_for_web(web: bool) {
    COMPILING_FOR_WEB.store(web, Ordering::Relaxed);
}

/// Returns true when the compile in progress is a `--web` build.
pub fn compiling_for_web() -> bool {
    COMPILING_FOR_WEB.load(Ordering::Relaxed)
}

/// PHP request superglobals visible in every scope under `--web`.
pub const SUPERGLOBALS: &[&str] =
    &["_SERVER", "_GET", "_POST", "_COOKIE", "_REQUEST", "_ENV", "_FILES", "_SESSION"];

/// Returns true when `name` (without leading `$`) is a request superglobal.
pub fn is_superglobal(name: &str) -> bool {
    SUPERGLOBALS.contains(&name)
}

/// The superglobals PHP's CLI SAPI has already populated when the script starts.
///
/// Measured, not assumed — under `php -n`:
/// `$_SERVER` holds 67 entries, `$_GET`/`$_POST`/`$_COOKIE`/`$_FILES` are empty
/// ARRAYS, and `$_REQUEST`/`$_ENV`/`$_SESSION` do not exist at all (the first two
/// depend on `variables_order`/`request_order`, the third on `session_start()`).
///
/// The distinction is observable and both halves are pinned by tests: a CLI
/// `count($_SERVER)` must answer a number rather than raise `count(): Argument #1
/// ($value) must be of type Countable|array, null given`, while `isset($_SESSION)`
/// must stay false. Off-web these names are ordinary top-level locals — nothing
/// pre-initializes the shared `_eir_global_*` storage — so a program that never
/// mentions one pays nothing for it.
pub const CLI_POPULATED_SUPERGLOBALS: &[&str] = &["_SERVER", "_GET", "_POST", "_COOKIE", "_FILES"];

/// The shared type of every request superglobal: a string-keyed associative
/// array of heterogeneous (Mixed) values.
pub fn superglobal_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}

/// Gives a CLI program the superglobals PHP's CLI SAPI would already have created,
/// by prepending `$_SERVER = [];` (and so on) for every one the source SPELLS.
///
/// Off-web these names are ordinary top-level locals: nothing pre-initializes the
/// shared `_eir_global_*` storage, so a bare read used to yield `null` and every
/// consumer inherited that — silently for `$_SERVER['x']`, and loudly once
/// `count()` started raising PHP's TypeError for a non-countable argument.
///
/// Seeding is driven by what the program mentions, so a source that never names
/// one emits nothing for it and the binary does not grow. The assignment is an
/// ordinary top-level store, which `contextualize_local_assignment` already types
/// as `AssocArray{Str, Mixed}` for these names, so the empty literal lands on hash
/// storage rather than indexed storage.
///
/// `$_REQUEST`, `$_ENV` and `$_SESSION` are deliberately absent from the seeded
/// set — see `CLI_POPULATED_SUPERGLOBALS`.
pub fn seed_cli_populated_superglobals(
    program: crate::parser::ast::Program,
) -> crate::parser::ast::Program {
    use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

    // Under `--web` the request prelude OWNS these names: it stores the real request
    // data into `_eir_global_*` before the script body runs. Prepending an empty
    // literal there would wipe the request.
    if compiling_for_web() {
        return program;
    }
    let spelled = crate::prelude_prune::usage::collect(&program).variables;
    let seeds: Vec<Stmt> = CLI_POPULATED_SUPERGLOBALS
        .iter()
        .filter(|name| spelled.contains(**name))
        .map(|name| {
            let span = crate::span::Span::synthetic();
            Stmt::new(
                StmtKind::Assign {
                    name: (*name).to_string(),
                    value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), span),
                },
                span,
            )
        })
        .collect();
    if seeds.is_empty() {
        return program;
    }
    // One `Synthetic` group rather than loose statements, so nothing that reasons
    // about the first top-level statement (a `declare`, an include-once guard) sees
    // a different one than the source wrote.
    let mut seeded = Vec::with_capacity(program.len() + 1);
    seeded.push(Stmt::new(StmtKind::Synthetic(seeds), crate::span::Span::synthetic()));
    seeded.extend(program);
    seeded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses PHP and returns the names the seeding pass prepends, in order.
    fn seeded_names(source: &str) -> Vec<String> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let seeded = seed_cli_populated_superglobals(program);
        match seeded.first().map(|stmt| &stmt.kind) {
            Some(StmtKind::Synthetic(seeds)) => seeds
                .iter()
                .filter_map(|stmt| match &stmt.kind {
                    StmtKind::Assign { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Pay-for-use is what lets this land on a branch about binary size: a program
    /// that never spells a superglobal must emit nothing for one. The end-to-end
    /// tests cannot see this — they only observe the names that ARE used.
    #[test]
    fn a_program_that_names_no_superglobal_is_left_alone() {
        assert!(seeded_names("<?php echo 1;").is_empty());
    }

    /// Only the name the source spells is seeded, not the whole set.
    #[test]
    fn only_the_superglobals_the_source_spells_are_seeded() {
        assert_eq!(seeded_names("<?php echo count($_GET);"), vec!["_GET".to_string()]);
    }

    /// `$_REQUEST`, `$_ENV` and `$_SESSION` are never seeded even when spelled: PHP's
    /// CLI SAPI does not create them, so `isset()` on them must stay false.
    #[test]
    fn superglobals_the_cli_does_not_create_are_never_seeded() {
        let source = "<?php echo isset($_SESSION), isset($_ENV), isset($_REQUEST);";
        assert!(seeded_names(source).is_empty());
    }

    /// A `--web` compile must be left completely alone: the request prelude owns
    /// these names there, and an empty literal would wipe the request.
    #[test]
    fn a_web_compile_is_left_alone() {
        set_compiling_for_web(true);
        let seeded = seeded_names("<?php echo count($_GET);");
        set_compiling_for_web(false);
        assert!(seeded.is_empty());
    }
}
