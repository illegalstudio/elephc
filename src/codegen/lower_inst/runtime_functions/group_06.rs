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

/// Lowers a target owned by bounded dispatch group 06, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::StreamFilterPrepend => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_filter_attach(ctx, inst, "stream_filter_prepend")
        }),
        RuntimeFnId::StreamFilterRegister => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_filter_register(ctx, inst)
        }),
        RuntimeFnId::StreamFilterRemove => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_filter_remove(ctx, inst)
        }),
        RuntimeFnId::StreamGetContents => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_get_contents(ctx, inst)
        }),
        RuntimeFnId::StreamGetFilters => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_get_filters(ctx, inst)
        }),
        RuntimeFnId::StreamGetLine => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_get_line(ctx, inst)
        }),
        RuntimeFnId::StreamGetMetaData => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_get_meta_data(ctx, inst)
        }),
        RuntimeFnId::StreamGetTransports => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_get_transports(ctx, inst)
        }),
        RuntimeFnId::StreamGetWrappers => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_get_wrappers(ctx, inst)
        }),
        RuntimeFnId::StreamIsLocal => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_is_local(ctx, inst)
        }),
        RuntimeFnId::StreamIsatty => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_isatty(ctx, inst)
        }),
        RuntimeFnId::StreamResolveIncludePath => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_resolve_include_path(ctx, inst)
        }),
        RuntimeFnId::StreamSelect => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_select(ctx, inst)
        }),
        RuntimeFnId::StreamSetBlocking => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_set_blocking(ctx, inst)
        }),
        RuntimeFnId::StreamSetChunkSize => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_set_chunk_size(ctx, inst)
        }),
        RuntimeFnId::StreamSetReadBuffer => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_set_buffer(ctx, inst)
        }),
        RuntimeFnId::StreamSetTimeout => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_set_timeout(ctx, inst)
        }),
        RuntimeFnId::StreamSetWriteBuffer => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_set_buffer(ctx, inst)
        }),
        RuntimeFnId::StreamSocketAccept => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_accept(ctx, inst)
        }),
        RuntimeFnId::StreamSocketClient => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_client(ctx, inst)
        }),
        RuntimeFnId::StreamSocketEnableCrypto => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_enable_crypto(ctx, inst)
        }),
        RuntimeFnId::StreamSocketGetName => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_get_name(ctx, inst)
        }),
        RuntimeFnId::StreamSocketPair => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_pair(ctx, inst)
        }),
        RuntimeFnId::StreamSocketRecvfrom => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_recvfrom(ctx, inst)
        }),
        RuntimeFnId::StreamSocketSendto => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_sendto(ctx, inst)
        }),
        RuntimeFnId::StreamSocketServer => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_server(ctx, inst)
        }),
        RuntimeFnId::StreamSocketShutdown => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_socket_shutdown(ctx, inst)
        }),
        RuntimeFnId::StreamSupportsLock => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_supports_lock(ctx, inst)
        }),
        RuntimeFnId::StreamWrapperRegister => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_wrapper_register(ctx, inst)
        }),
        RuntimeFnId::StreamWrapperRestore => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_wrapper_restore(ctx, inst)
        }),
        RuntimeFnId::StreamWrapperUnregister => Some({
            crate::codegen::lower_inst::builtins::io::lower_stream_wrapper_unregister(ctx, inst)
        }),
        RuntimeFnId::Symlink => Some({
            crate::codegen::lower_inst::builtins::io::lower_symlink(ctx, inst)
        }),
        RuntimeFnId::SysGetTempDir => Some({
            crate::codegen::lower_inst::builtins::io::lower_sys_get_temp_dir(ctx, inst)
        }),
        RuntimeFnId::Tempnam => Some({
            crate::codegen::lower_inst::builtins::io::lower_tempnam(ctx, inst)
        }),
        RuntimeFnId::Tmpfile => Some({
            crate::codegen::lower_inst::builtins::io::lower_tmpfile(ctx, inst)
        }),
        _ => None,
    }
}
