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

/// Lowers a target owned by bounded dispatch group 03, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::DiskFreeSpace => Some({
            crate::codegen::lower_inst::builtins::io::lower_disk_free_space(ctx, inst)
        }),
        RuntimeFnId::DiskTotalSpace => Some({
            crate::codegen::lower_inst::builtins::io::lower_disk_total_space(ctx, inst)
        }),
        RuntimeFnId::Fclose => Some({
            crate::codegen::lower_inst::builtins::io::lower_fclose(ctx, inst)
        }),
        RuntimeFnId::Fdatasync => Some({
            crate::codegen::lower_inst::builtins::io::lower_fdatasync(ctx, inst)
        }),
        RuntimeFnId::Feof => Some({
            crate::codegen::lower_inst::builtins::io::lower_feof(ctx, inst)
        }),
        RuntimeFnId::Fflush => Some({
            crate::codegen::lower_inst::builtins::io::lower_fflush(ctx, inst)
        }),
        RuntimeFnId::Fgetc => Some({
            crate::codegen::lower_inst::builtins::io::lower_fgetc(ctx, inst)
        }),
        RuntimeFnId::Fgetcsv => Some({
            crate::codegen::lower_inst::builtins::io::lower_fgetcsv(ctx, inst)
        }),
        RuntimeFnId::Fgets => Some({
            crate::codegen::lower_inst::builtins::io::lower_fgets(ctx, inst)
        }),
        RuntimeFnId::File => Some({
            crate::codegen::lower_inst::builtins::io::lower_file(ctx, inst)
        }),
        RuntimeFnId::FileExists => Some({
            crate::codegen::lower_inst::builtins::io::lower_file_exists(ctx, inst)
        }),
        RuntimeFnId::FileGetContents => Some({
            crate::codegen::lower_inst::builtins::io::lower_file_get_contents(ctx, inst)
        }),
        RuntimeFnId::FilePutContents => Some({
            crate::codegen::lower_inst::builtins::io::lower_file_put_contents(ctx, inst)
        }),
        RuntimeFnId::Fileatime => Some({
            crate::codegen::lower_inst::builtins::io::lower_fileatime(ctx, inst)
        }),
        RuntimeFnId::Filectime => Some({
            crate::codegen::lower_inst::builtins::io::lower_filectime(ctx, inst)
        }),
        RuntimeFnId::Filegroup => Some({
            crate::codegen::lower_inst::builtins::io::lower_filegroup(ctx, inst)
        }),
        RuntimeFnId::Fileinode => Some({
            crate::codegen::lower_inst::builtins::io::lower_fileinode(ctx, inst)
        }),
        RuntimeFnId::Filemtime => Some({
            crate::codegen::lower_inst::builtins::io::lower_filemtime(ctx, inst)
        }),
        RuntimeFnId::Fileowner => Some({
            crate::codegen::lower_inst::builtins::io::lower_fileowner(ctx, inst)
        }),
        RuntimeFnId::Fileperms => Some({
            crate::codegen::lower_inst::builtins::io::lower_fileperms(ctx, inst)
        }),
        RuntimeFnId::Filesize => Some({
            crate::codegen::lower_inst::builtins::io::lower_filesize(ctx, inst)
        }),
        RuntimeFnId::Filetype => Some({
            crate::codegen::lower_inst::builtins::io::lower_filetype(ctx, inst)
        }),
        RuntimeFnId::Flock => Some({
            crate::codegen::lower_inst::builtins::io::lower_flock(ctx, inst)
        }),
        RuntimeFnId::Fnmatch => Some({
            crate::codegen::lower_inst::builtins::io::lower_fnmatch(ctx, inst)
        }),
        RuntimeFnId::Fopen => Some({
            crate::codegen::lower_inst::builtins::io::lower_fopen(ctx, inst)
        }),
        RuntimeFnId::Fpassthru => Some({
            crate::codegen::lower_inst::builtins::io::lower_fpassthru(ctx, inst)
        }),
        RuntimeFnId::Fprintf => Some({
            crate::codegen::lower_inst::builtins::io::lower_fprintf(ctx, inst)
        }),
        RuntimeFnId::Fputcsv => Some({
            crate::codegen::lower_inst::builtins::io::lower_fputcsv(ctx, inst)
        }),
        RuntimeFnId::Fread => Some({
            crate::codegen::lower_inst::builtins::io::lower_fread(ctx, inst)
        }),
        RuntimeFnId::Fscanf => Some({
            crate::codegen::lower_inst::builtins::io::lower_fscanf(ctx, inst)
        }),
        RuntimeFnId::Fseek => Some({
            crate::codegen::lower_inst::builtins::io::lower_fseek(ctx, inst)
        }),
        RuntimeFnId::Fsockopen => Some({
            crate::codegen::lower_inst::builtins::io::lower_fsockopen(ctx, inst)
        }),
        RuntimeFnId::Fstat => Some({
            crate::codegen::lower_inst::builtins::io::lower_fstat(ctx, inst)
        }),
        RuntimeFnId::Fsync => Some({
            crate::codegen::lower_inst::builtins::io::lower_fsync(ctx, inst)
        }),
        RuntimeFnId::Ftell => Some({
            crate::codegen::lower_inst::builtins::io::lower_ftell(ctx, inst)
        }),
        _ => None,
    }
}
