//! Purpose:
//! Lowers scalar extern function calls from EIR into the target C ABI.
//! Covers the Phase 04 parity path for non-string, non-callable FFI calls.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Source-order evaluation already happened during AST-to-EIR lowering; this
//!   module only materializes precomputed SSA values into C ABI locations.
//! - String and callable extern parameters require cleanup/trampoline handling
//!   and remain explicit unsupported cases until their dedicated lowering lands.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{ExternDecl, ExternParamDecl, Instruction, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers an EIR extern call to a platform-mangled C symbol call.
pub(super) fn lower_extern_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let decl = extern_decl(ctx, inst)?.clone();
    validate_extern_shape(&decl)?;
    if inst.operands.len() != decl.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "extern call to {} with {} args for {} params",
            decl.name,
            inst.operands.len(),
            decl.params.len()
        )));
    }

    let param_types = decl
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect::<Vec<_>>();
    for (idx, param) in decl.params.iter().enumerate() {
        let value = expect_operand(inst, idx)?;
        materialize_extern_arg(ctx, value, param)?;
        abi::emit_push_result_value(ctx.emitter, &param.php_type);
    }

    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &param_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let symbol = ctx.emitter.target.extern_symbol(&decl.name);
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    normalize_extern_return(ctx, &decl.return_php_type)?;
    store_if_result(ctx, inst)
}

/// Returns the extern declaration addressed by the instruction's function-name immediate.
fn extern_decl<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a ExternDecl> {
    let data = expect_data(inst)?;
    let name = ctx.function_name_data(data)?;
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    ctx.module
        .extern_decls
        .iter()
        .find(|decl| crate::names::php_symbol_key(decl.name.trim_start_matches('\\')) == key)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown extern function {}", name)))
}

/// Rejects extern features whose ABI cleanup or trampoline lowering is not implemented yet.
fn validate_extern_shape(decl: &ExternDecl) -> Result<()> {
    for param in &decl.params {
        validate_supported_extern_type(&decl.name, &param.php_type, "parameter")?;
    }
    validate_supported_extern_type(&decl.name, &decl.return_php_type, "return")
}

/// Validates one extern-facing PHP type against the scalar subset this module supports.
fn validate_supported_extern_type(name: &str, ty: &PhpType, position: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Void
        | PhpType::Pointer(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "extern {} {} type {:?}",
            name,
            position,
            other
        ))),
    }
}

/// Loads and coerces an SSA value into the ABI result register expected by an extern parameter.
fn materialize_extern_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param: &ExternParamDecl,
) -> Result<()> {
    let target_ty = param.php_type.codegen_repr();
    let actual_ty = ctx.value_php_type(value)?;
    match (&target_ty, actual_ty.codegen_repr()) {
        (PhpType::Pointer(_), PhpType::Void) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        (PhpType::Float, PhpType::Int | PhpType::Bool) => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        (PhpType::Int, PhpType::Bool) | (PhpType::Bool, PhpType::Int) => {
            ctx.load_value_to_result(value)?;
        }
        (expected, actual) if extern_codegen_types_match(expected, &actual) => {
            ctx.load_value_to_result(value)?;
        }
        (expected, actual) => {
            return Err(CodegenIrError::unsupported(format!(
                "extern parameter ${} expects {:?}, got {:?}",
                param.name,
                expected,
                actual
            )))
        }
    }
    Ok(())
}

/// Returns true when two scalar extern ABI types can be passed without extra conversion.
fn extern_codegen_types_match(expected: &PhpType, actual: &PhpType) -> bool {
    match (expected, actual) {
        (PhpType::Pointer(_), PhpType::Pointer(_)) => true,
        _ => expected == actual,
    }
}

/// Normalizes extern return registers before storing the EIR result.
fn normalize_extern_return(ctx: &mut FunctionContext<'_>, return_ty: &PhpType) -> Result<()> {
    match return_ty.codegen_repr() {
        PhpType::Void => Ok(()),
        PhpType::Int => {
            emit_sign_extend_i32_result(ctx);
            Ok(())
        }
        PhpType::Bool | PhpType::Float | PhpType::Pointer(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "extern return type {:?}",
            other
        ))),
    }
}

/// Sign-extends a C `int` return into the target integer result register.
fn emit_sign_extend_i32_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sxtw x0, w0");                             // sign-extend the C int return into PHP's 64-bit integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movsxd rax, eax");                         // sign-extend the C int return into PHP's 64-bit integer result
        }
    }
}
