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
mod expect_array_arg;
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

pub(crate) use count_empty::*;
pub(crate) use expect_array_arg::*;
pub(in crate::codegen::lower_inst) use eval_facade::*;
pub(crate) use function_queries::*;
pub(crate) use member_queries::*;
pub(crate) use scalar_metadata::*;
use shared::*;
pub(crate) use type_predicates::*;

const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";

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
