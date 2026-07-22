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

/// Lowers a target owned by bounded dispatch group 10, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::MbStrlen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_strlen(ctx, inst)
        }),
        RuntimeFnId::Md5 => Some({
            crate::codegen::lower_inst::builtins::strings::lower_md5(ctx, inst)
        }),
        RuntimeFnId::NumberFormat => Some({
            crate::codegen::lower_inst::builtins::strings::lower_number_format(ctx, inst)
        }),
        RuntimeFnId::Ord => Some({
            crate::codegen::lower_inst::builtins::strings::lower_ord(ctx, inst)
        }),
        RuntimeFnId::Printf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_printf(ctx, inst)
        }),
        RuntimeFnId::Rtrim => Some({
            crate::codegen::lower_inst::builtins::strings::lower_trim_like(
                    ctx,
                    inst,
                    "rtrim",
                    "__rt_rtrim",
                    "__rt_rtrim_mask",
                )
        }),
        RuntimeFnId::Sha1 => Some({
            crate::codegen::lower_inst::builtins::strings::lower_sha1(ctx, inst)
        }),
        RuntimeFnId::Sprintf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_sprintf(ctx, inst)
        }),
        RuntimeFnId::Sscanf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_sscanf(ctx, inst)
        }),
        RuntimeFnId::StrContains => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_contains(ctx, inst)
        }),
        RuntimeFnId::StrEndsWith => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "str_ends_with",
                    "__rt_str_ends_with",
                )
        }),
        RuntimeFnId::StrIreplace => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_replace(
                    ctx,
                    inst,
                    "str_ireplace",
                    "__rt_str_ireplace",
                )
        }),
        RuntimeFnId::StrPad => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_pad(ctx, inst)
        }),
        RuntimeFnId::StrRepeat => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_repeat(ctx, inst)
        }),
        RuntimeFnId::StrReplace => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_replace(
                    ctx,
                    inst,
                    "str_replace",
                    "__rt_str_replace",
                )
        }),
        RuntimeFnId::StrSplit => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_split(ctx, inst)
        }),
        RuntimeFnId::StrStartsWith => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "str_starts_with",
                    "__rt_str_starts_with",
                )
        }),
        RuntimeFnId::Strcasecmp => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "strcasecmp",
                    "__rt_strcasecmp",
                )
        }),
        RuntimeFnId::Strcmp => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "strcmp",
                    "__rt_strcmp",
                )
        }),
        RuntimeFnId::Strpos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position(
                    ctx,
                    inst,
                    "strpos",
                    "__rt_strpos",
                )
        }),
        RuntimeFnId::Strrpos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position(
                    ctx,
                    inst,
                    "strrpos",
                    "__rt_strrpos",
                )
        }),
        RuntimeFnId::Strstr => Some({
            crate::codegen::lower_inst::builtins::strings::lower_strstr(ctx, inst)
        }),
        RuntimeFnId::Substr => Some({
            crate::codegen::lower_inst::builtins::strings::lower_substr(ctx, inst)
        }),
        RuntimeFnId::SubstrReplace => Some({
            crate::codegen::lower_inst::builtins::strings::lower_substr_replace(ctx, inst)
        }),
        RuntimeFnId::Trim => Some({
            crate::codegen::lower_inst::builtins::strings::lower_trim_like(
                    ctx,
                    inst,
                    "trim",
                    "__rt_trim",
                    "__rt_trim_mask",
                )
        }),
        RuntimeFnId::Ucfirst => Some({
            crate::codegen::lower_inst::builtins::strings::lower_ucfirst(ctx, inst)
        }),
        RuntimeFnId::Ucwords => Some({
            crate::codegen::lower_inst::builtins::strings::lower_unary_string_runtime(
                    ctx,
                    inst,
                    "ucwords",
                    "__rt_ucwords",
                )
        }),
        RuntimeFnId::Vprintf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_vprintf(ctx, inst)
        }),
        RuntimeFnId::Vsprintf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_vsprintf(ctx, inst)
        }),
        RuntimeFnId::Wordwrap => Some({
            crate::codegen::lower_inst::builtins::strings::lower_wordwrap(ctx, inst)
        }),
        RuntimeFnId::ElephcGmmktimeRaw => Some({
            crate::codegen::lower_inst::builtins::system::lower_gmmktime(ctx, inst)
        }),
        RuntimeFnId::ElephcMktimeRaw => Some({
            crate::codegen::lower_inst::builtins::system::lower_mktime(ctx, inst)
        }),
        RuntimeFnId::ElephcStrtotimeRaw => Some({
            crate::codegen::lower_inst::builtins::system::lower_elephc_strtotime_raw(ctx, inst)
        }),
        RuntimeFnId::Checkdate => Some({
            crate::codegen::lower_inst::builtins::system::lower_checkdate(ctx, inst)
        }),
        RuntimeFnId::ClassAttributeArgs => Some({
            crate::codegen::lower_inst::builtins::attributes::lower_class_attribute_args(ctx, inst)
        }),
        _ => None,
    }
}
