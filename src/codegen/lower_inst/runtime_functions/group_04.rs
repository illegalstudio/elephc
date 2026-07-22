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

/// Lowers a target owned by bounded dispatch group 04, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::Ftruncate => Some({
            crate::codegen::lower_inst::builtins::io::lower_ftruncate(ctx, inst)
        }),
        RuntimeFnId::Fwrite => Some({
            crate::codegen::lower_inst::builtins::io::lower_fwrite(ctx, inst)
        }),
        RuntimeFnId::Getcwd => Some({
            crate::codegen::lower_inst::builtins::io::lower_getcwd(ctx, inst)
        }),
        RuntimeFnId::Gethostbyaddr => Some({
            crate::codegen::lower_inst::builtins::io::lower_gethostbyaddr(ctx, inst)
        }),
        RuntimeFnId::Gethostbyname => Some({
            crate::codegen::lower_inst::builtins::io::lower_gethostbyname(ctx, inst)
        }),
        RuntimeFnId::Gethostname => Some({
            crate::codegen::lower_inst::builtins::io::lower_gethostname(ctx, inst)
        }),
        RuntimeFnId::Getprotobyname => Some({
            crate::codegen::lower_inst::builtins::io::lower_getprotobyname(ctx, inst)
        }),
        RuntimeFnId::Getprotobynumber => Some({
            crate::codegen::lower_inst::builtins::io::lower_getprotobynumber(ctx, inst)
        }),
        RuntimeFnId::Getservbyname => Some({
            crate::codegen::lower_inst::builtins::io::lower_getservbyname(ctx, inst)
        }),
        RuntimeFnId::Getservbyport => Some({
            crate::codegen::lower_inst::builtins::io::lower_getservbyport(ctx, inst)
        }),
        RuntimeFnId::Glob => Some({
            crate::codegen::lower_inst::builtins::io::lower_glob(ctx, inst)
        }),
        RuntimeFnId::HashFile => Some({
            crate::codegen::lower_inst::builtins::io::lower_hash_file(ctx, inst)
        }),
        RuntimeFnId::IsDir => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_dir(ctx, inst)
        }),
        RuntimeFnId::IsExecutable => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_executable(ctx, inst)
        }),
        RuntimeFnId::IsFile => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_file(ctx, inst)
        }),
        RuntimeFnId::IsLink => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_link(ctx, inst)
        }),
        RuntimeFnId::IsReadable => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_readable(ctx, inst)
        }),
        RuntimeFnId::IsWritable => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_writable(ctx, inst)
        }),
        RuntimeFnId::IsWriteable => Some({
            crate::codegen::lower_inst::builtins::io::lower_is_writeable(ctx, inst)
        }),
        RuntimeFnId::Lchgrp => Some({
            crate::codegen::lower_inst::builtins::io::lower_lchgrp(ctx, inst)
        }),
        RuntimeFnId::Lchown => Some({
            crate::codegen::lower_inst::builtins::io::lower_lchown(ctx, inst)
        }),
        RuntimeFnId::Link => Some({
            crate::codegen::lower_inst::builtins::io::lower_link(ctx, inst)
        }),
        RuntimeFnId::Linkinfo => Some({
            crate::codegen::lower_inst::builtins::io::lower_linkinfo(ctx, inst)
        }),
        RuntimeFnId::Lstat => Some({
            crate::codegen::lower_inst::builtins::io::lower_lstat(ctx, inst)
        }),
        RuntimeFnId::Mkdir => Some({
            crate::codegen::lower_inst::builtins::io::lower_mkdir(ctx, inst)
        }),
        RuntimeFnId::ObClean => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_clean(ctx, inst)
        }),
        RuntimeFnId::ObEndClean => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_end_clean(ctx, inst)
        }),
        RuntimeFnId::ObEndFlush => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_end_flush(ctx, inst)
        }),
        RuntimeFnId::ObFlush => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_flush(ctx, inst)
        }),
        RuntimeFnId::ObGetClean => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_clean(ctx, inst)
        }),
        RuntimeFnId::ObGetContents => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_contents(ctx, inst)
        }),
        RuntimeFnId::ObGetFlush => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_flush(ctx, inst)
        }),
        RuntimeFnId::ObGetLength => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_length(ctx, inst)
        }),
        RuntimeFnId::ObGetLevel => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_level(ctx, inst)
        }),
        RuntimeFnId::ObGetStatus => Some({
            crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_status(ctx, inst)
        }),
        _ => None,
    }
}
