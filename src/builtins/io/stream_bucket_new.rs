//! Purpose:
//! Home of the PHP `stream_bucket_new` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Mixed`.


builtin! {
    name: "stream_bucket_new",
    area: Io,
    params: [stream: Mixed, buffer: Str],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamBucketNew,
    ),
    summary: "Creates a new bucket for use in a stream filter.",
    php_manual: "function.stream-bucket-new",
}
