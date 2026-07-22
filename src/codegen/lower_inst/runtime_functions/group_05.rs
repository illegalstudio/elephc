//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 05, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::ObImplicitFlush => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_implicit_flush(ctx, inst)
        }),
        RuntimeFnId::ObListHandlers => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_list_handlers(ctx, inst)
        }),
        RuntimeFnId::ObStart => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_start(ctx, inst)
        }),
        RuntimeFnId::Opendir => Some({
            crate::codegen::lower_inst::builtins::io::lower_opendir(ctx, inst)
        }),
        RuntimeFnId::Pathinfo => Some({
            crate::codegen::lower_inst::builtins::io::lower_pathinfo(ctx, inst)
        }),
        RuntimeFnId::Pclose => Some({
            crate::codegen::lower_inst::builtins::io::lower_pclose(ctx, inst)
        }),
        RuntimeFnId::Pfsockopen => Some({
            crate::codegen::lower_inst::builtins::io::lower_fsockopen(ctx, inst)
        }),
        RuntimeFnId::Popen => Some({
            crate::codegen::lower_inst::builtins::io::lower_popen(ctx, inst)
        }),
        RuntimeFnId::PrintR => Some({
            crate::codegen::lower_inst::builtins::debug::lower_print_r(ctx, inst)
        }),
        RuntimeFnId::Readdir => Some({
            crate::codegen::lower_inst::builtins::io::lower_readdir(ctx, inst)
        }),
        RuntimeFnId::Readfile => Some({
            crate::codegen::lower_inst::builtins::io::lower_readfile(ctx, inst)
        }),
        RuntimeFnId::Readline => Some({
            crate::codegen::lower_inst::builtins::io::lower_readline(ctx, inst)
        }),
        RuntimeFnId::Readlink => Some({
            crate::codegen::lower_inst::builtins::io::lower_readlink(ctx, inst)
        }),
        RuntimeFnId::Realpath => Some({
            crate::codegen::lower_inst::builtins::io::lower_realpath(ctx, inst)
        }),
        RuntimeFnId::RealpathCacheGet => Some({
            crate::codegen::lower_inst::builtins::io::lower_realpath_cache_get(ctx, inst)
        }),
        RuntimeFnId::RealpathCacheSize => Some({
            crate::codegen::lower_inst::builtins::io::lower_realpath_cache_size(ctx, inst)
        }),
        RuntimeFnId::Rename => Some({
            crate::codegen::lower_inst::builtins::io::lower_rename(ctx, inst)
        }),
        RuntimeFnId::Rewind => Some({
            crate::codegen::lower_inst::builtins::io::lower_rewind(ctx, inst)
        }),
        RuntimeFnId::Rewinddir => Some({
            crate::codegen::lower_inst::builtins::io::lower_rewinddir(ctx, inst)
        }),
        RuntimeFnId::Rmdir => Some({
            crate::codegen::lower_inst::builtins::io::lower_rmdir(ctx, inst)
        }),
        RuntimeFnId::Scandir => Some({
            crate::codegen::lower_inst::builtins::io::lower_scandir(ctx, inst)
        }),
        RuntimeFnId::Stat => Some({
            crate::codegen::lower_inst::builtins::io::lower_stat(ctx, inst)
        }),
        RuntimeFnId::StreamBucketAppend => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_bucket_append_or_prepend(ctx, inst)
        }),
        RuntimeFnId::StreamBucketMakeWriteable => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_bucket_make_writeable(ctx, inst)
        }),
        RuntimeFnId::StreamBucketNew => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_bucket_new(ctx, inst)
        }),
        RuntimeFnId::StreamBucketPrepend => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_bucket_append_or_prepend(ctx, inst)
        }),
        RuntimeFnId::StreamContextCreate => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_create(ctx, inst)
        }),
        RuntimeFnId::StreamContextGetDefault => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_get_default(ctx, inst)
        }),
        RuntimeFnId::StreamContextGetOptions => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_get_options(ctx, inst)
        }),
        RuntimeFnId::StreamContextGetParams => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_get_params(ctx, inst)
        }),
        RuntimeFnId::StreamContextSetDefault => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_set_default(ctx, inst)
        }),
        RuntimeFnId::StreamContextSetOption => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_set_option(ctx, inst)
        }),
        RuntimeFnId::StreamContextSetParams => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_context_set_params(ctx, inst)
        }),
        RuntimeFnId::StreamCopyToStream => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_copy_to_stream(ctx, inst)
        }),
        RuntimeFnId::StreamFilterAppend => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_filter_attach(ctx, inst, "stream_filter_append")
        }),
        _ => None,
    }
}
