//! Purpose:
//! Lowers PHP extension-introspection builtins whose results are baked from the compiler's
//! supported extension surface.
//!
//! Called from:
//! - Typed runtime-function dispatch in `crate::codegen::lower_inst::runtime_functions`.
//!
//! Key details:
//! - `get_extension_funcs("date")` preserves php-src declaration order and casing.
//! - Literal and runtime extension names use the same case-insensitive matching rule.
//! - Both the array and `false` arms are boxed because PHP declares `array|false`.

use crate::codegen::{abi, emit_box_current_value_as_mixed, CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// php-src declaration-order inventory returned by `get_extension_funcs("date")`.
const DATE_EXTENSION_FUNCTIONS: &[&str] = &[
    "strtotime",
    "date",
    "idate",
    "gmdate",
    "mktime",
    "gmmktime",
    "checkdate",
    "strftime",
    "gmstrftime",
    "time",
    "localtime",
    "getdate",
    "date_create",
    "date_create_immutable",
    "date_create_from_format",
    "date_create_immutable_from_format",
    "date_parse",
    "date_parse_from_format",
    "date_get_last_errors",
    "date_format",
    "date_modify",
    "date_add",
    "date_sub",
    "date_timezone_get",
    "date_timezone_set",
    "date_offset_get",
    "date_diff",
    "date_time_set",
    "date_date_set",
    "date_isodate_set",
    "date_timestamp_set",
    "date_timestamp_get",
    "timezone_open",
    "timezone_name_get",
    "timezone_name_from_abbr",
    "timezone_offset_get",
    "timezone_transitions_get",
    "timezone_location_get",
    "timezone_identifiers_list",
    "timezone_abbreviations_list",
    "timezone_version_get",
    "date_interval_create_from_date_string",
    "date_interval_format",
    "date_default_timezone_set",
    "date_default_timezone_get",
    "date_sunrise",
    "date_sunset",
    "date_sun_info",
];

/// Lowers `get_extension_funcs($extension)` to an ordered function array or boxed `false`.
pub(crate) fn lower_get_extension_funcs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "get_extension_funcs", 1)?;
    let extension = expect_operand(inst, 0)?;
    if let Some(name) = super::types::optional_const_string_operand(ctx, extension)? {
        if name.eq_ignore_ascii_case("date") {
            emit_date_extension_function_array(ctx)?;
        } else {
            emit_boxed_false(ctx);
        }
    } else {
        lower_dynamic_get_extension_funcs(ctx, extension)?;
    }
    store_if_result(ctx, inst)
}

/// Emits the date extension function inventory as an owned boxed `Mixed` array.
fn emit_date_extension_function_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let functions = DATE_EXTENSION_FUNCTIONS
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    super::types::emit_string_array(ctx, &functions)?;
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Str)));
    Ok(())
}

/// Emits PHP `false` as the boxed fallback of `get_extension_funcs()`.
fn emit_boxed_false(ctx: &mut FunctionContext<'_>) {
    super::types::emit_bool_result(ctx, false);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Lowers a runtime extension name against the baked, case-insensitive date inventory.
fn lower_dynamic_get_extension_funcs(
    ctx: &mut FunctionContext<'_>,
    extension: ValueId,
) -> Result<()> {
    if ctx.value_php_type(extension)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(
            "get_extension_funcs with non-string dynamic name",
        ));
    }
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(extension, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let matched_label = ctx.next_label("get_extension_funcs_date");
    let done_label = ctx.next_label("get_extension_funcs_done");
    super::emit_branch_if_saved_string_matches_ci(ctx, b"date", &matched_label);
    emit_boxed_false(ctx);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&matched_label);
    emit_date_extension_function_array(ctx)?;

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}
