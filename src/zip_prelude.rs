//! Purpose:
//! Declares PHP's `ZipArchive` surface and its `elephc_zip` bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile_config` when the program names `ZipArchive` or
//!   the bridge is forced with `--with-zip`.
//!
//! Key details:
//! - The archive parser/writer is the pure-Rust ZIP engine in `elephc-phar`;
//!   the bridge crate owns opaque in-memory handles and mutations.
//! - The prelude is parsed before name resolution, so its `extern` block makes
//!   the checker require `elephc_zip` only for programs that use the class.
//! - `openString()`/`closeString()` are PHP 8.6-only; the marker block is
//!   stripped on earlier profiles so the class matches that runtime.

use crate::parser::ast::Program;

/// The `ZipArchive` PHP source, including the 8.6 method marker block.
const ZIP_PRELUDE_SRC: &str = include_str!("zip_prelude/zip_archive.php");

/// Returns the prelude for the active PHP profile.
fn source() -> String {
    let php86 = crate::codegen_support::compile_php_version()
        >= crate::php_version::PhpVersion::Php86;
    if php86 {
        return ZIP_PRELUDE_SRC.to_string();
    }
    let mut out = String::with_capacity(ZIP_PRELUDE_SRC.len());
    let mut skipping = false;
    for line in ZIP_PRELUDE_SRC.lines() {
        match line.trim() {
            "/* __PHP86_BEGIN__ */" => skipping = true,
            "/* __PHP86_END__ */" => skipping = false,
            _ if !skipping => {
                out.push_str(line);
                out.push('\n');
            }
            _ => {}
        }
    }
    out
}

/// Returns whether `usage` reaches `ZipArchive`.
fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.classes.iter().any(|name| name == "ziparchive")
        || usage.literals.iter().any(|name| name == "ziparchive")
}

/// Prepends the `ZipArchive` prelude when the program reaches it or it is forced.
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
    let source = source();
    let tokens = crate::lexer::tokenize(&source).expect("ZipArchive prelude must tokenize");
    let mut declarations =
        crate::parser::parse_internal(&tokens).expect("ZipArchive prelude must parse");
    inventory.record_program("zip", &declarations);
    declarations.append(&mut program);
    declarations
}
