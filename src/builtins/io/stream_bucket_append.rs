//! Purpose:
//! Home of the PHP `stream_bucket_append` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Void`.


builtin! {
    name: "stream_bucket_append",
    area: Io,
    params: [brigade: Mixed, bucket: Mixed],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamBucketAppend,
    ),
    summary: "Appends a bucket to the brigade.",
    php_manual: "function.stream-bucket-append",
}
