//! Purpose:
//! Home of the internal `__elephc_phar_archive_valid` PHAR intrinsic: its declaration,
//! checker contract, and semantic target. Compiler-synthesized; not PHP-visible.
//!
//! Called from:
//! - The synthetic `Phar`/`PharData` constructor, which must reject an existing archive
//!   that the current `phar.require_hash` policy cannot authenticate.
//!
//! Key details:
//! - A missing path reports true so the constructor can still create a new archive.
//! - The bridge links `elephc_phar` because the validity probe lives in that crate.

builtin! {
    contract: "__elephc_phar_archive_valid",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPharArchiveValid,
    ),
}
