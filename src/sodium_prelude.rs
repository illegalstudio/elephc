//! Purpose:
//! Declares the first slice of PHP's `sodium_*` surface and its `elephc_sodium`
//! bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile_config` when the program names a `sodium_*`
//!   function/constant or `SodiumException`, or the bridge is forced with
//!   `--with-sodium`.
//!
//! Key details:
//! - The bridge is pure Rust and implements NaCl's XSalsa20-Poly1305
//!   `crypto_secretbox`, byte-compatible with libsodium; no system libsodium is
//!   required.
//! - The prelude is parsed before name resolution, so its `extern` block makes
//!   the checker require `elephc_sodium` only for programs that use the surface.
//! - Length validation and PHP's exact `SodiumException` diagnostics live in
//!   PHP, before the binary-safe bridge call.

use crate::parser::ast::Program;

/// The `sodium_*` PHP source and bridge externs.
const SODIUM_PRELUDE_SRC: &str = include_str!("sodium_prelude/sodium.php");

/// Returns whether `usage` reaches the Sodium surface.
fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage
        .functions
        .iter()
        .any(|name| name.starts_with("sodium_"))
        || usage
            .classes
            .iter()
            .any(|name| name == "sodiumexception")
        || usage
            .constants
            .iter()
            .any(|name| name.starts_with("SODIUM_"))
        || usage.literals.iter().any(|literal| {
            let literal = literal.to_ascii_lowercase();
            literal.starts_with("sodium_") || literal == "sodiumexception"
        })
}

/// Prepends the Sodium prelude when the program reaches it or it is forced.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force {
        let usage = crate::prelude_prune::usage::collect(&program);
        if !requested(&usage) {
            return program;
        }
    }
    let tokens = crate::lexer::tokenize(SODIUM_PRELUDE_SRC)
        .expect("Sodium prelude must tokenize");
    let mut declarations =
        crate::parser::parse_internal(&tokens).expect("Sodium prelude must parse");
    inventory.record_program("sodium", &declarations);
    declarations.append(&mut program);
    declarations
}
