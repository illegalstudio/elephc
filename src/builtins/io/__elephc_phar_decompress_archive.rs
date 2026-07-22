//! Purpose:
//! Home of the internal `__elephc_phar_decompress_archive` PHAR intrinsic: its declaration, checker contract, and semantic target. Compiler-synthesized; not PHP-visible.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `internal: true` keeps it out of PHP-visible builtin name sets and
//!   `function_exists()`; it is reachable only through compiler-generated PHAR bodies.
//! - The `check` hook links the `elephc_phar` bridge library (a mandatory side effect);
//!   argument inference is handled by the registry common path, so the hook does not
//!   call `infer_type`.


builtin! {
    name: "__elephc_phar_decompress_archive",
    area: Io,
    params: [src: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPharDecompressArchive,
    ),
    summary: "Decompresses a PHAR archive to a new path.",
    internal: true,
}
