//! Purpose:
//! Owns target-aware builtin emitters plus the small set of PHP language constructs
//! represented by EIR `LanguageConstructCall` instructions.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime conversions reuse existing target-aware helpers instead of duplicating parsing logic.
//! - Selected Mixed predicates inspect the boxed runtime tag through shared predicate lowering.

use std::collections::BTreeSet;

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_support::data_section::DataWord;
use crate::ir::{Immediate, Instruction, Op, PhpTypePredicate, ValueDef, ValueId};
use crate::names::{define_seen_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::checker::builtins::{
    is_php_visible_builtin_function_for_profile, supported_builtin_function_names_for_profile,
};
use crate::types::{ClassInfo, PhpType};

use super::super::context::FunctionContext;
use super::{
    class_implements_interface, exceptions, expect_data, expect_operand,
    instruction_strict_php_profile, load_value_to_first_int_arg,
    lower_instance_runtime_intrinsic, lower_runtime_object_method_call, predicates,
    runtime_backed_instance_intrinsic, store_if_result,
};
use crate::codegen::{CodegenIrError, Result};

pub(crate) mod attributes;
pub(crate) mod arrays;
pub(crate) mod bcmath;
pub(crate) mod buffers;
pub(crate) mod class_relations;
pub(crate) mod ctype;
pub(crate) mod debug;
mod eval;
mod eval_facade;
pub(crate) mod io;
mod isset;
mod count_empty;
mod function_queries;
mod member_queries;
mod scalar_metadata;
mod shared;
mod type_predicates;
pub(crate) mod is_numeric;
pub(crate) mod json;
pub(crate) mod math;
pub(crate) mod object_props;
pub(crate) mod openssl;
pub(crate) mod output_buffering;
pub(crate) mod pointers;
pub(crate) mod regex;
pub(crate) mod round_mode;
pub(crate) mod serialize;
pub(crate) mod spl;
pub(crate) mod system;
pub(crate) mod strings;
pub(crate) mod types;

pub(in crate::codegen::lower_inst) use eval_facade::*;
pub(crate) use function_queries::*;
pub(crate) use member_queries::*;
pub(crate) use scalar_metadata::*;
use shared::*;
pub(crate) use type_predicates::*;

const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";

/// Lowers `count()` while preserving native SimpleXML wrapper dispatch for boxed receivers.
pub(crate) fn lower_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return count_empty::lower_count(ctx, inst);
    }
    let value = expect_operand(inst, 0)?;
    if !matches!(
        ctx.value_php_type(value)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return count_empty::lower_count(ctx, inst);
    }
    lower_mixed_count(ctx, inst, value)
}

/// Dispatches boxed SimpleXML wrappers through their native count handler before generic Mixed count.
fn lower_mixed_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
) -> Result<()> {
    let count_key = php_symbol_key("count");
    let mut candidates = super::mixed_simplexml_candidates(ctx);
    candidates.retain(|candidate| {
        let Some(class_info) = ctx.module.class_infos.get(&candidate.class_name) else {
            return false;
        };
        class_info
            .method_impl_classes
            .get(&count_key)
            .map(String::as_str)
            .unwrap_or(candidate.class_name.as_str())
            .eq_ignore_ascii_case("SimpleXMLElement")
    });
    if candidates.is_empty() {
        return lower_generic_mixed_count(ctx, inst, value);
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let fallback_label = ctx.next_label("mixed_count_fallback");
    let done_label = ctx.next_label("mixed_count_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_count_{}",
                super::label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    super::emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &fallback_label);
    super::emit_mixed_simplexml_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &fallback_label,
    );

    let opcode = crate::internal_extensions::operation_registry()
        .object_handler("simplexml", "count")
        .ok_or_else(|| CodegenIrError::invalid_module("missing SimpleXML count object handler"))?
        .opcode;
    for label in &match_labels {
        ctx.emitter.label(label);
        super::internal_extensions::lower_mixed_receiver_internal_extension_call(
            ctx,
            inst,
            receiver_reg,
            opcode,
            &PhpType::Int,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fallback_label);
    lower_generic_mixed_count(ctx, inst, value)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Preserves the established boxed array/hash/SPL count fallback for non-SimpleXML values.
fn lower_generic_mixed_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
    store_if_result(ctx, inst)
}

/// Lowers one compiler-resident PHP language construct by its canonical name.
pub(super) fn lower_language_construct_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let name = ctx.function_name_data(expect_data(inst)?)?;
    let key = php_symbol_key(name.trim_start_matches('\\'));
    match key.as_str() {
        "eval" => eval::lower_eval(ctx, inst),
        "empty" => lower_empty(ctx, inst),
        "unset" => types::lower_unset_builtin(ctx, inst),
        "isset" => isset::lower_isset(ctx, inst),
        "exit" | "die" => system::lower_exit(ctx, inst),
        _ => Err(CodegenIrError::unsupported(format!("language construct {}", name))),
    }
}

/// Lowers an EIR native indexed-array `isset($array[$offset])` probe.
pub(super) fn lower_array_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    isset::lower_array_isset(ctx, inst)
}

/// Lowers an EIR native associative-array `isset($hash[$key])` probe.
pub(super) fn lower_hash_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    isset::lower_hash_isset(ctx, inst)
}
