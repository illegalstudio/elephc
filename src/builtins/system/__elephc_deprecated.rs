//! Purpose:
//! Home of the internal `__elephc_deprecated` builtin: raises one php `Deprecated:`
//! diagnostic whose text is a run-time string, so an injected prelude body can report a
//! deprecation in its own name.
//!
//! Called from:
//! - The synthetic `SplFileObject` bodies (`src/types/checker/builtin_spl_classes/filesystem.rs`),
//!   which own php 8.4's three `$escape` deprecations on `fgetcsv`, `fputcsv` and `setCsvControl`.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` cannot hide it from an injected
//!   body and no user program can call it.
//! - `trigger_error()` does not exist in elephc, and the deprecation the CSV BUILTINS raise is
//!   chosen by the builtin's own name and argument count (`emit_csv_escape_deprecation` in
//!   `src/codegen/lower_inst/builtins/io/stream_file_ops.rs`). Neither reaches a body that
//!   always forwards `$escape` to the builtin, which is why the primitive is a builtin of its
//!   own rather than another arm of that emitter.
//! - The argument is the COMPLETE line, `Deprecated: ` prefix and trailing newline included:
//!   `__rt_diag_warning` accumulates pieces and flushes on the newline, appends php's
//!   ` in <file> on line <n>` tail, and honours `@` suppression — so a body that raises this
//!   is silenced exactly where php silences its own.

builtin! {
    contract: "__elephc_deprecated",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcDeprecated,
    ),
}
