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

/// Lowers a target owned by bounded dispatch group 02, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::GetClass => Some({
            crate::codegen::lower_inst::builtins::types::lower_class_name_lookup(ctx, inst, "get_class")
        }),
        RuntimeFnId::GetDeclaredClasses => Some({
            crate::codegen::lower_inst::builtins::types::lower_get_declared_names(
                    ctx,
                    inst,
                    "get_declared_classes",
                )
        }),
        RuntimeFnId::GetDeclaredInterfaces => Some({
            crate::codegen::lower_inst::builtins::types::lower_get_declared_names(
                    ctx,
                    inst,
                    "get_declared_interfaces",
                )
        }),
        RuntimeFnId::GetDeclaredTraits => Some({
            crate::codegen::lower_inst::builtins::types::lower_get_declared_names(
                    ctx,
                    inst,
                    "get_declared_traits",
                )
        }),
        RuntimeFnId::GetParentClass => Some({
            crate::codegen::lower_inst::builtins::types::lower_class_name_lookup(
                    ctx,
                    inst,
                    "get_parent_class",
                )
        }),
        RuntimeFnId::InterfaceExists => Some({
            crate::codegen::lower_inst::builtins::lower_class_like_exists(
                    ctx,
                    inst,
                    "interface_exists",
                )
        }),
        RuntimeFnId::IsA => Some({
            crate::codegen::lower_inst::builtins::types::lower_is_a_relation(ctx, inst, "is_a")
        }),
        RuntimeFnId::IsSubclassOf => Some({
            crate::codegen::lower_inst::builtins::types::lower_is_a_relation(
                    ctx,
                    inst,
                    "is_subclass_of",
                )
        }),
        RuntimeFnId::PregReplaceCallback => Some({
            crate::codegen::lower_inst::builtins::regex::lower_preg_replace_callback(ctx, inst)
        }),
        RuntimeFnId::TraitExists => Some({
            crate::codegen::lower_inst::builtins::lower_class_like_exists(ctx, inst, "trait_exists")
        }),
        RuntimeFnId::ElephcPharBzip2Archive => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_bzip2_archive(ctx, inst)
        }),
        RuntimeFnId::ElephcPharDecompressArchive => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_decompress_archive(ctx, inst)
        }),
        RuntimeFnId::ElephcPharGetFileMetadata => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_get_file_metadata(ctx, inst)
        }),
        RuntimeFnId::ElephcPharGetMetadata => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_get_metadata(ctx, inst)
        }),
        RuntimeFnId::ElephcPharGetSignatureHash => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_get_signature_hash(ctx, inst)
        }),
        RuntimeFnId::ElephcPharGetSignatureType => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_get_signature_type(ctx, inst)
        }),
        RuntimeFnId::ElephcPharGetStub => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_get_stub(ctx, inst)
        }),
        RuntimeFnId::ElephcPharGzipArchive => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_gzip_archive(ctx, inst)
        }),
        RuntimeFnId::ElephcPharListEntries => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_list_entries(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSetCompression => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_set_compression(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSetFileMetadata => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_set_file_metadata(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSetMetadata => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_set_metadata(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSetStub => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_set_stub(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSetZipPassword => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_set_zip_password(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSignHash => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_sign_hash(ctx, inst)
        }),
        RuntimeFnId::ElephcPharSignOpenssl => Some({
            crate::codegen::lower_inst::builtins::io::lower_elephc_phar_sign_openssl(ctx, inst)
        }),
        RuntimeFnId::Basename => Some({
            crate::codegen::lower_inst::builtins::io::lower_basename(ctx, inst)
        }),
        RuntimeFnId::Chdir => Some({
            crate::codegen::lower_inst::builtins::io::lower_chdir(ctx, inst)
        }),
        RuntimeFnId::Chgrp => Some({
            crate::codegen::lower_inst::builtins::io::lower_chgrp(ctx, inst)
        }),
        RuntimeFnId::Chmod => Some({
            crate::codegen::lower_inst::builtins::io::lower_chmod(ctx, inst)
        }),
        RuntimeFnId::Chown => Some({
            crate::codegen::lower_inst::builtins::io::lower_chown(ctx, inst)
        }),
        RuntimeFnId::Clearstatcache => Some({
            crate::codegen::lower_inst::builtins::io::lower_clearstatcache(ctx, inst)
        }),
        RuntimeFnId::Closedir => Some({
            crate::codegen::lower_inst::builtins::io::lower_closedir(ctx, inst)
        }),
        RuntimeFnId::Copy => Some({
            crate::codegen::lower_inst::builtins::io::lower_copy(ctx, inst)
        }),
        RuntimeFnId::Dirname => Some({
            crate::codegen::lower_inst::builtins::io::lower_dirname(ctx, inst)
        }),
        _ => None,
    }
}
