//! Purpose:
//! Home of the PHP `stream_bucket_make_writeable` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the single argument and returns `Mixed`.


builtin! {
    name: "stream_bucket_make_writeable",
    area: Io,
    params: [brigade: Mixed],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamBucketMakeWriteable,
    ),
    summary: "Returns a bucket object from the brigade for use in a stream filter.",
    php_manual: "function.stream-bucket-make-writeable",
}
