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

/// Lowers a target owned by bounded dispatch group 11, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::ClassAttributeNames => Some({
            crate::codegen::lower_inst::builtins::attributes::lower_class_attribute_names(ctx, inst)
        }),
        RuntimeFnId::ClassGetAttributes => Some({
            crate::codegen::lower_inst::builtins::attributes::lower_class_get_attributes(ctx, inst)
        }),
        RuntimeFnId::Date => Some({
            crate::codegen::lower_inst::builtins::system::lower_date(ctx, inst)
        }),
        RuntimeFnId::DateDefaultTimezoneGet => Some({
            crate::codegen::lower_inst::builtins::system::lower_date_default_timezone_get(ctx, inst)
        }),
        RuntimeFnId::DateDefaultTimezoneSet => Some({
            crate::codegen::lower_inst::builtins::system::lower_date_default_timezone_set(ctx, inst)
        }),
        RuntimeFnId::Define => Some({
            crate::codegen::lower_inst::builtins::lower_define(ctx, inst)
        }),
        RuntimeFnId::Defined => Some({
            crate::codegen::lower_inst::builtins::lower_defined(ctx, inst)
        }),
        RuntimeFnId::Exec => Some({
            crate::codegen::lower_inst::builtins::system::lower_exec(ctx, inst)
        }),
        RuntimeFnId::Getdate => Some({
            crate::codegen::lower_inst::builtins::system::lower_getdate(ctx, inst)
        }),
        RuntimeFnId::Getenv => Some({
            crate::codegen::lower_inst::builtins::system::lower_getenv(ctx, inst)
        }),
        RuntimeFnId::Gmdate => Some({
            crate::codegen::lower_inst::builtins::system::lower_gmdate(ctx, inst)
        }),
        RuntimeFnId::Gmmktime => Some({
            crate::codegen::lower_inst::builtins::system::lower_gmmktime(ctx, inst)
        }),
        RuntimeFnId::Header => Some({
            crate::codegen::lower_inst::builtins::system::lower_header(ctx, inst)
        }),
        RuntimeFnId::Hrtime => Some({
            crate::codegen::lower_inst::builtins::system::lower_hrtime(ctx, inst)
        }),
        RuntimeFnId::HttpResponseCode => Some({
            crate::codegen::lower_inst::builtins::system::lower_http_response_code(ctx, inst)
        }),
        RuntimeFnId::JsonDecode => Some({
            crate::codegen::lower_inst::builtins::json::lower_json_decode(ctx, inst)
        }),
        RuntimeFnId::JsonEncode => Some({
            crate::codegen::lower_inst::builtins::json::lower_json_encode(ctx, inst)
        }),
        RuntimeFnId::JsonLastError => Some({
            crate::codegen::lower_inst::builtins::json::lower_json_last_error(ctx, inst)
        }),
        RuntimeFnId::JsonLastErrorMsg => Some({
            crate::codegen::lower_inst::builtins::json::lower_json_last_error_msg(ctx, inst)
        }),
        RuntimeFnId::JsonValidate => Some({
            crate::codegen::lower_inst::builtins::json::lower_json_validate(ctx, inst)
        }),
        RuntimeFnId::Localtime => Some({
            crate::codegen::lower_inst::builtins::system::lower_localtime(ctx, inst)
        }),
        RuntimeFnId::Microtime => Some({
            crate::codegen::lower_inst::builtins::system::lower_microtime(ctx, inst)
        }),
        RuntimeFnId::Mktime => Some({
            crate::codegen::lower_inst::builtins::system::lower_mktime(ctx, inst)
        }),
        RuntimeFnId::Passthru => Some({
            crate::codegen::lower_inst::builtins::system::lower_passthru(ctx, inst)
        }),
        RuntimeFnId::PhpUname => Some({
            crate::codegen::lower_inst::builtins::system::lower_php_uname(ctx, inst)
        }),
        RuntimeFnId::Phpversion => Some({
            crate::codegen::lower_inst::builtins::lower_phpversion(ctx, inst)
        }),
        RuntimeFnId::PregMatch => Some({
            crate::codegen::lower_inst::builtins::regex::lower_preg_match(ctx, inst)
        }),
        RuntimeFnId::PregMatchAll => Some({
            crate::codegen::lower_inst::builtins::regex::lower_preg_match_all(ctx, inst)
        }),
        RuntimeFnId::PregReplace => Some({
            crate::codegen::lower_inst::builtins::regex::lower_preg_replace(ctx, inst)
        }),
        RuntimeFnId::PregSplit => Some({
            crate::codegen::lower_inst::builtins::regex::lower_preg_split(ctx, inst)
        }),
        RuntimeFnId::Putenv => Some({
            crate::codegen::lower_inst::builtins::system::lower_putenv(ctx, inst)
        }),
        RuntimeFnId::Serialize => Some({
            crate::codegen::lower_inst::builtins::serialize::lower_serialize(ctx, inst)
        }),
        RuntimeFnId::ShellExec => Some({
            crate::codegen::lower_inst::builtins::system::lower_shell_exec(ctx, inst)
        }),
        RuntimeFnId::Sleep => Some({
            crate::codegen::lower_inst::builtins::system::lower_sleep(ctx, inst)
        }),
        RuntimeFnId::Strtotime => Some({
            crate::codegen::lower_inst::builtins::system::lower_strtotime(ctx, inst)
        }),
        _ => None,
    }
}
