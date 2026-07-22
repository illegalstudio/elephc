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

/// Lowers a target owned by bounded dispatch group 09, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::SplAutoloadUnregister => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_bool(
                    ctx,
                    inst,
                    "spl_autoload_unregister",
                )
        }),
        RuntimeFnId::SplClasses => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_classes(ctx, inst)
        }),
        RuntimeFnId::SplObjectHash => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_object_hash(ctx, inst)
        }),
        RuntimeFnId::SplObjectId => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_object_id(ctx, inst)
        }),
        RuntimeFnId::Chop => Some({
            crate::codegen::lower_inst::builtins::strings::lower_trim_like(
                    ctx,
                    inst,
                    "chop",
                    "__rt_rtrim",
                    "__rt_rtrim_mask",
                )
        }),
        RuntimeFnId::Chr => Some({
            crate::codegen::lower_inst::builtins::strings::lower_chr(ctx, inst)
        }),
        RuntimeFnId::Crc32 => Some({
            crate::codegen::lower_inst::builtins::strings::lower_crc32(ctx, inst)
        }),
        RuntimeFnId::CtypeAlnum => Some({
            crate::codegen::lower_inst::builtins::ctype::lower_ctype_alnum(ctx, inst)
        }),
        RuntimeFnId::CtypeAlpha => Some({
            crate::codegen::lower_inst::builtins::ctype::lower_ctype_alpha(ctx, inst)
        }),
        RuntimeFnId::CtypeDigit => Some({
            crate::codegen::lower_inst::builtins::ctype::lower_ctype_digit(ctx, inst)
        }),
        RuntimeFnId::CtypeSpace => Some({
            crate::codegen::lower_inst::builtins::ctype::lower_ctype_space(ctx, inst)
        }),
        RuntimeFnId::Explode => Some({
            crate::codegen::lower_inst::builtins::strings::lower_explode(ctx, inst)
        }),
        RuntimeFnId::GraphemeStrrev => Some({
            crate::codegen::lower_inst::builtins::strings::lower_grapheme_strrev(ctx, inst)
        }),
        RuntimeFnId::Gzcompress => Some({
            crate::codegen::lower_inst::builtins::strings::lower_gzcompress(ctx, inst)
        }),
        RuntimeFnId::Gzdeflate => Some({
            crate::codegen::lower_inst::builtins::strings::lower_gzdeflate(ctx, inst)
        }),
        RuntimeFnId::Gzinflate => Some({
            crate::codegen::lower_inst::builtins::strings::lower_gzinflate(ctx, inst)
        }),
        RuntimeFnId::Gzuncompress => Some({
            crate::codegen::lower_inst::builtins::strings::lower_gzuncompress(ctx, inst)
        }),
        RuntimeFnId::Hash => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash(ctx, inst)
        }),
        RuntimeFnId::HashAlgos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_algos(ctx, inst)
        }),
        RuntimeFnId::HashCopy => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_copy(ctx, inst)
        }),
        RuntimeFnId::HashEquals => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_equals(ctx, inst)
        }),
        RuntimeFnId::HashFinal => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_final(ctx, inst)
        }),
        RuntimeFnId::HashHmac => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_hmac(ctx, inst)
        }),
        RuntimeFnId::HashInit => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_init(ctx, inst)
        }),
        RuntimeFnId::HashUpdate => Some({
            crate::codegen::lower_inst::builtins::strings::lower_hash_update(ctx, inst)
        }),
        RuntimeFnId::Htmlentities => Some({
            crate::codegen::lower_inst::builtins::strings::lower_html_escape(ctx, inst, "htmlentities")
        }),
        RuntimeFnId::Htmlspecialchars => Some({
            crate::codegen::lower_inst::builtins::strings::lower_html_escape(ctx, inst, "htmlspecialchars")
        }),
        RuntimeFnId::Implode => Some({
            crate::codegen::lower_inst::builtins::strings::lower_implode(ctx, inst)
        }),
        RuntimeFnId::InetNtop => Some({
            crate::codegen::lower_inst::builtins::strings::lower_inet(
                    ctx,
                    inst,
                    "inet_ntop",
                    "__rt_inet_ntop",
                )
        }),
        RuntimeFnId::InetPton => Some({
            crate::codegen::lower_inst::builtins::strings::lower_inet(
                    ctx,
                    inst,
                    "inet_pton",
                    "__rt_inet_pton",
                )
        }),
        RuntimeFnId::Ip2long => Some({
            crate::codegen::lower_inst::builtins::strings::lower_ip2long(ctx, inst)
        }),
        RuntimeFnId::Lcfirst => Some({
            crate::codegen::lower_inst::builtins::strings::lower_lcfirst(ctx, inst)
        }),
        RuntimeFnId::Long2ip => Some({
            crate::codegen::lower_inst::builtins::strings::lower_long2ip(ctx, inst)
        }),
        RuntimeFnId::Ltrim => Some({
            crate::codegen::lower_inst::builtins::strings::lower_trim_like(
                    ctx,
                    inst,
                    "ltrim",
                    "__rt_ltrim",
                    "__rt_ltrim_mask",
                )
        }),
        RuntimeFnId::MbEregMatch => Some({
            crate::codegen::lower_inst::builtins::regex::lower_mb_ereg_match(ctx, inst)
        }),
        _ => None,
    }
}
