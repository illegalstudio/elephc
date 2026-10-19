//! Purpose:
//! Dispatches the iconv and `ext/curl` group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - This group owns the iconv and curl extension families.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{Instruction, RuntimeFnId};

/// Lowers a target owned by bounded dispatch group 14, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::Iconv => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv(ctx, inst)
        }),
        RuntimeFnId::IconvGetEncoding => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_get_encoding(ctx, inst)
        }),
        RuntimeFnId::IconvMimeDecode => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_mime_decode(ctx, inst)
        }),
        RuntimeFnId::IconvMimeDecodeHeaders => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_mime_decode_headers(ctx, inst)
        }),
        RuntimeFnId::IconvMimeEncode => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_mime_encode(ctx, inst)
        }),
        RuntimeFnId::IconvSetEncoding => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_set_encoding(ctx, inst)
        }),
        RuntimeFnId::IconvStrlen => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_strlen(ctx, inst)
        }),
        RuntimeFnId::IconvStrpos => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_strpos(ctx, inst)
        }),
        RuntimeFnId::IconvStrrpos => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_strrpos(ctx, inst)
        }),
        RuntimeFnId::IconvSubstr => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_substr(ctx, inst)
        }),
        RuntimeFnId::CurlEasyBody => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_body(ctx, inst)
        }),
        RuntimeFnId::CurlEasyErrno => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_errno(ctx, inst)
        }),
        RuntimeFnId::CurlEasyError => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_error(ctx, inst)
        }),
        RuntimeFnId::CurlEasyGetinfoLong => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_getinfo_long(ctx, inst)
        }),
        RuntimeFnId::CurlEasyGetinfoDouble => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_getinfo_double(ctx, inst)
        }),
        RuntimeFnId::CurlEasyStrOp => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_str_op(ctx, inst)
        }),
        RuntimeFnId::CurlEasyCopy => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_copy(ctx, inst)
        }),
        RuntimeFnId::CurlEasyPause => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_pause(ctx, inst)
        }),
        RuntimeFnId::CurlEasyReset => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_reset(ctx, inst)
        }),
        RuntimeFnId::CurlEasyUpkeep => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_upkeep(ctx, inst)
        }),
        RuntimeFnId::CurlStrerror => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_strerror(ctx, inst)
        }),
        RuntimeFnId::CurlEasyInit => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_init(ctx, inst)
        }),
        RuntimeFnId::CurlEasyPerform => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_perform(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetCallback => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_set_callback(ctx, inst)
        }),
        RuntimeFnId::CurlAdapterAddr => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_adapter_addr(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetoptLong => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_setopt_long(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetoptStr => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_setopt_str(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetoptSlist => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_setopt_slist(ctx, inst)
        }),
        RuntimeFnId::CurlOptionKind => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_option_kind(ctx, inst)
        }),
        RuntimeFnId::IncludeWarning => Some(lower_include_warning(ctx, inst)),
        RuntimeFnId::CurlSetoptUnsupportedWarning => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_setopt_unsupported_warning(
                ctx, inst,
            )
        }),
        RuntimeFnId::CurlVersion => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_version(ctx, inst)
        }),
        RuntimeFnId::CurlEasyId => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_id(ctx, inst)
        }),
        RuntimeFnId::CurlMultiInit => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_init(ctx, inst)
        }),
        RuntimeFnId::CurlMultiAdd => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_add(ctx, inst)
        }),
        RuntimeFnId::CurlMultiRemove => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_remove(ctx, inst)
        }),
        RuntimeFnId::CurlMultiExec => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_exec(ctx, inst)
        }),
        RuntimeFnId::CurlMultiSelect => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_select(ctx, inst)
        }),
        RuntimeFnId::CurlMultiInfoRead => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_info_read(ctx, inst)
        }),
        RuntimeFnId::CurlMultiSetopt => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_setopt(ctx, inst)
        }),
        RuntimeFnId::CurlMultiErrno => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_errno(ctx, inst)
        }),
        RuntimeFnId::CurlMultiStrerror => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_strerror(ctx, inst)
        }),
        RuntimeFnId::CurlMultiSetoptUnsupportedWarning => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_multi_setopt_unsupported_warning(
                ctx, inst,
            )
        }),
        RuntimeFnId::CurlShareInit => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_share_init(ctx, inst)
        }),
        RuntimeFnId::CurlShareSetopt => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_share_setopt(ctx, inst)
        }),
        RuntimeFnId::CurlShareErrno => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_share_errno(ctx, inst)
        }),
        RuntimeFnId::CurlShareStrerror => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_share_strerror(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetShare => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_set_share(ctx, inst)
        }),
        RuntimeFnId::CurlShareInitPersistent => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_share_init_persistent(
                ctx, inst,
            )
        }),
        RuntimeFnId::CurlMimeNew => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_mime_new(ctx, inst)
        }),
        RuntimeFnId::CurlMimeAddPart => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_mime_add_part(ctx, inst)
        }),
        RuntimeFnId::CurlMimePartField => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_mime_part_field(ctx, inst)
        }),
        RuntimeFnId::CurlMimePost => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_mime_post(ctx, inst)
        }),
        RuntimeFnId::CurlMimeAbort => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_mime_abort(ctx, inst)
        }),
        _ => None,
    }
}

/// Lowers `__elephc_include_warning($message)`: `Warning: <message>` through the suppressible
/// diagnostic channel, which derives `E_WARNING` from the prefix and honours handlers,
/// `error_reporting` and `@` (see `codegen_support::runtime::warning_dispatch`).
fn lower_include_warning(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    use crate::codegen::lower_inst::runtime_class_messages::{diagnostic_fragment_regs, emit_static_diagnostic};
    let message = crate::codegen::lower_inst::expect_operand(inst, 0)?;
    emit_static_diagnostic(ctx, "Warning: ", false);
    let (ptr_reg, len_reg) = diagnostic_fragment_regs(ctx.emitter);
    ctx.load_string_value_to_regs(message, ptr_reg, len_reg)?;
    crate::codegen::abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    emit_static_diagnostic(ctx, "\n", true);
    crate::codegen::lower_inst::store_if_result(ctx, inst)
}
