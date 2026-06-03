//! Purpose:
//! Lowers the first scalar PHP builtin calls emitted as EIR `BuiltinCall` instructions.
//! Covers concrete scalar casts, type predicates, and string length without Mixed dispatch.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Only statically concrete scalar representations are handled here; Mixed/Union paths stay unsupported.
//! - Runtime conversions reuse existing target-aware helpers instead of duplicating parsing logic.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::checker::builtins::canonical_builtin_function_name;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, predicates, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

mod is_numeric;

/// Lowers a scalar builtin call by matching the canonical PHP function name.
pub(super) fn lower_builtin_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let name = ctx.function_name_data(expect_data(inst)?)?;
    let key = php_symbol_key(name.trim_start_matches('\\'));
    match key.as_str() {
        "pi" => lower_pi(ctx, inst),
        "phpversion" => lower_phpversion(ctx, inst),
        "strlen" => lower_strlen(ctx, inst),
        "count" => lower_count(ctx, inst),
        "intval" => lower_intval(ctx, inst),
        "floatval" => lower_floatval(ctx, inst),
        "boolval" => lower_boolval(ctx, inst),
        "defined" => lower_defined(ctx, inst),
        "function_exists" => lower_function_exists(ctx, inst),
        "is_callable" => lower_is_callable(ctx, inst),
        "is_int" => lower_static_type_predicate(ctx, inst, "is_int", PhpType::Int),
        "is_float" => lower_static_type_predicate(ctx, inst, "is_float", PhpType::Float),
        "is_bool" => lower_static_type_predicate(ctx, inst, "is_bool", PhpType::Bool),
        "is_null" => lower_is_null_builtin(ctx, inst),
        "is_string" => lower_static_type_predicate(ctx, inst, "is_string", PhpType::Str),
        "is_numeric" => is_numeric::lower_is_numeric(ctx, inst),
        _ => Err(CodegenIrError::unsupported(format!("builtin call {}", name))),
    }
}

/// Lowers `pi()` as the same data-section float constant used by the legacy backend.
fn lower_pi(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "pi", 0)?;
    let label = ctx.data.add_float(std::f64::consts::PI);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x9", &label);                                     // load the page address that contains the M_PI floating constant
            ctx.emitter.ldr_lo12("d0", "x9", &label);                          // load the M_PI floating constant into the floating result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movsd xmm0, QWORD PTR [rip + {}]", label)); // load the M_PI floating constant into the floating result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `phpversion()` as the compiler package version string.
fn lower_phpversion(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "phpversion", 0)?;
    let (label, len) = ctx.data.add_string(env!("CARGO_PKG_VERSION").as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `defined("NAME")` for compile-time string constant names.
fn lower_defined(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "defined", 1)?;
    let value = expect_operand(inst, 0)?;
    let constant_name = const_string_operand(ctx, value)?;
    emit_static_bool(ctx, ctx.has_global_name(&constant_name));
    store_if_result(ctx, inst)
}

/// Lowers `function_exists("name")` for compile-time string names.
fn lower_function_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "function_exists", 1)?;
    let value = expect_operand(inst, 0)?;
    let function_name = const_string_operand(ctx, value)?;
    if let Some(group_name) = ctx.function_variant_group_name(&function_name) {
        emit_variant_function_exists(ctx, &group_name);
    } else {
        let exists = ctx.function_by_name(&function_name).is_some()
            || ctx.has_extern_function(&function_name)
            || canonical_builtin_function_name(function_name.trim_start_matches('\\')).is_some();
        emit_static_bool(ctx, exists);
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_callable(value)` for static strings and concrete scalar types.
fn lower_is_callable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_callable", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Callable => emit_static_bool(ctx, true),
        PhpType::Str => {
            let function_name = const_string_operand(ctx, value)?;
            if function_name.contains("::") {
                return Err(CodegenIrError::unsupported(
                    "is_callable static-method string lookup",
                ));
            }
            emit_static_bool(ctx, callable_name_exists(ctx, &function_name));
        }
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Void | PhpType::Never => {
            emit_static_bool(ctx, false);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "is_callable for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a runtime check for whether an include-loaded function variant is active.
fn emit_variant_function_exists(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let active_symbol = crate::names::function_variant_active_symbol(function_name);
    ctx.data.add_comm(active_symbol.clone(), 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &active_symbol, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // test whether an include has activated this function variant
            ctx.emitter.instruction(&format!("cset {}, ne", result_reg));       // return true only when a function variant is active
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // test whether an include has activated this function variant
            ctx.emitter.instruction("setne al");                                // return true only when a function variant is active
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Lowers `count(array)` for concrete array values by reading the runtime length header.
fn lower_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "count", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
            store_if_result(ctx, inst)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "count for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers `strlen(string)` by returning the loaded string-result length register.
fn lower_strlen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "strlen", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "strlen for PHP type {:?}",
            ty
        )));
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `intval()` for concrete scalar operands.
fn lower_intval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "intval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_float_result_to_int_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "intval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `floatval()` for concrete scalar operands.
fn lower_floatval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "floatval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "floatval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`.
fn lower_boolval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "boolval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "boolval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a static `is_*` predicate for concrete non-Mixed values.
fn lower_static_type_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    expected: PhpType,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::Mixed {
        return Err(CodegenIrError::unsupported(format!("{} for PHP type Mixed", name)));
    }
    emit_static_bool(ctx, ty == expected);
    store_if_result(ctx, inst)
}

/// Lowers `is_null()` for concrete scalar values.
fn lower_is_null_builtin(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_null", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::Mixed {
        return Err(CodegenIrError::unsupported("is_null for PHP type Mixed"));
    }
    emit_static_bool(ctx, matches!(ty, PhpType::Void | PhpType::Never));
    store_if_result(ctx, inst)
}

/// Emits a boolean immediate into the integer result register.
fn emit_static_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns true when a static callable name resolves to any known callable function.
fn callable_name_exists(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.function_variant_group_name(name).is_some()
        || ctx.function_by_name(name).is_some()
        || ctx.has_extern_function(name)
        || canonical_builtin_function_name(name.trim_start_matches('\\')).is_some()
}

/// Returns a string literal value defined by a `ConstStr` instruction.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-literal function name",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-literal function name",
        ));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "function_exists string literal has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Verifies that the builtin call has the expected number of lowered operands.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}
