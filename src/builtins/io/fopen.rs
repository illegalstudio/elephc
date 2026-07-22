//! Purpose:
//! Home of the PHP `fopen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` detects the URL scheme from a string-literal first argument and links
//!   the appropriate runtime libraries (`elephc_tls`, `z`, `bz2`, `elephc_phar`,
//!   `elephc_crypto`) at compile time. Non-literal paths conservatively link all
//!   PHAR and decompression libraries.
//! - Returns `Union(stream_resource, Bool)` via `returns: Mixed` because the union
//!   involves a resource type that the scalar `returns:` field cannot express.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does
//!   NOT re-infer them.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fopen",
    area: Io,
    params: [
        filename: Str,
        mode: Str,
        use_include_path: Bool = DefaultSpec::Bool(false),
        context: Mixed = DefaultSpec::Null
    ],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fopen,
    ),
    requirements: crate::builtins::semantics::fopen_requirements,
    summary: "Opens file or URL.",
    php_manual: "function.fopen",
}

/// Detects URL scheme from the filename literal and links the required runtime libraries.
///
/// A literal `https://` or `ftps://` URL links `elephc_tls`. A `compress.zlib://` scheme
/// links `z`. A `compress.bzip2://` scheme links `bz2`. A `phar://` URL in write mode
/// links `elephc_phar` and `elephc_crypto`. A non-literal path conservatively links
/// `elephc_phar`, `z`, and `bz2` because the scheme is unknown until run time.
/// Returns `Union(stream_resource, Bool)` for the success/false-on-failure PHP pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}
