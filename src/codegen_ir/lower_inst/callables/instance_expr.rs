//! Purpose:
//! Lowers EIR callable calls whose callable operand is a receiver-bound
//! instance-method first-class callable recovered from local SSA producers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::callables::lower_expr_call()`.
//!
//! Key details:
//! - The descriptor already retains the captured receiver; this lowering
//!   statically recovers that receiver and reuses the normal method-call ABI
//!   for both `expr_call` and variable-call `closure_call` opcodes.

use crate::codegen::abi;
use crate::ir::{BlockId, Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::{method_symbol, php_symbol_key};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{
    class_method_already_emitted, direct_call_stack_pad_bytes, emit_ref_arg_writebacks,
    materialize_method_call_args_with_receiver_reg_and_refs, store_call_result,
};
use crate::codegen_ir::{CodegenIrError, Result};

/// Resolved receiver-bound first-class callable target for `($callable)(...)`.
struct InstanceMethodExprCallTarget {
    entry_label: String,
    receiver: ValueId,
    receiver_ty: PhpType,
    param_types: Vec<PhpType>,
    ref_params: Vec<bool>,
    return_ty: PhpType,
}

/// Lowers `($fn)(...)` when `$fn` is a stored instance-method first-class callable.
pub(super) fn lower_instance_method_expr_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
) -> Result<()> {
    lower_instance_method_callable_call(ctx, inst, callable, "expr_call")
}

/// Lowers `$fn(...)` when `$fn` is a stored instance-method first-class callable.
pub(super) fn lower_instance_method_closure_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
) -> Result<()> {
    lower_instance_method_callable_call(ctx, inst, callable, "closure_call")
}

/// Lowers a receiver-bound instance-method callable through the normal method-call ABI.
fn lower_instance_method_callable_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    owner: &str,
) -> Result<()> {
    let target = instance_method_expr_call_target(ctx, callable, owner)?;
    let visible_args = inst.operands.iter().skip(1).copied().collect::<Vec<_>>();
    let mut operands = Vec::with_capacity(visible_args.len() + 1);
    operands.push(target.receiver);
    operands.extend(visible_args);
    if operands.len() != target.param_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} '{}' with {} operands for {} ABI params",
            owner,
            target.entry_label,
            operands.len(),
            target.param_types.len()
        )));
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    ctx.load_value_to_reg(target.receiver, receiver_reg)?;
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &target.receiver_ty,
        &operands,
        &target.param_types,
        &target.ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &target.entry_label);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &target.return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Resolves a callable operand back to the receiver-bound method descriptor that produced it.
fn instance_method_expr_call_target(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<InstanceMethodExprCallTarget> {
    let source = expr_callable_source_instruction(ctx, value, owner)?;
    let Some(Immediate::Data(data)) = source.immediate.as_ref() else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} first-class callable has no target data",
            owner
        )));
    };
    let target = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Err(CodegenIrError::unsupported(format!(
            "{} first-class callable '{}' is not a method target",
            owner,
            target
        )));
    };
    if receiver_label.trim_start_matches('\\') != "object" {
        return Err(CodegenIrError::unsupported(format!(
            "{} first-class callable '{}' is not receiver-bound",
            owner,
            target
        )));
    }
    let receiver = source.operands.first().copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "{} receiver-bound callable '{}' has no receiver operand",
            owner,
            target
        ))
    })?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let PhpType::Object(class_name) = receiver_ty.clone() else {
        return Err(CodegenIrError::unsupported(format!(
            "{} receiver-bound callable '{}' with receiver PHP type {:?}",
            owner,
            target,
            receiver_ty
        )));
    };
    let normalized_class = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized_class)
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "{} receiver-bound callable '{}' with unknown class '{}'",
            owner,
            target,
            normalized_class
        )))?;
    let method_key = php_symbol_key(method_name);
    let sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} receiver-bound callable '{}' with unknown method",
            owner,
            target
        ))
    })?;
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(normalized_class);
    if !class_method_already_emitted(ctx, impl_class, &method_key, false) {
        return Err(CodegenIrError::unsupported(format!(
            "{} receiver-bound callable '{}' without emitted EIR method body",
            owner,
            target
        )));
    }
    let mut param_types = Vec::with_capacity(sig.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(sig.ref_params.iter().copied());
    Ok(InstanceMethodExprCallTarget {
        entry_label: method_symbol(impl_class, &method_key),
        receiver,
        receiver_ty,
        param_types,
        ref_params,
        return_ty: sig.return_type.clone(),
    })
}

/// Returns the first-class callable producer for an expr-call operand.
fn expr_callable_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<&'a Instruction> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-static callable operand",
            owner
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op == Op::LoadLocal {
        return expr_callable_local_source_instruction(ctx, block, index, inst_ref, owner);
    }
    require_expr_callable_source(inst_ref, owner)
}

/// Resolves a local callable load to the last same-block store before that load.
fn expr_callable_local_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
    owner: &str,
) -> Result<&'a Instruction> {
    let Some(Immediate::LocalSlot(slot)) = load_inst.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} load_local callable has no local slot",
            owner
        )));
    };
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut stored = None;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            stored = inst_ref.operands.first().copied();
        }
    }
    let Some(stored) = stored else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with local callable operand that has no prior same-block store",
            owner
        )));
    };
    let Some(value_ref) = ctx.function.value(stored) else {
        return Err(CodegenIrError::missing_entry("value", stored.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with local callable operand from non-instruction value",
            owner
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    require_expr_callable_source(inst_ref, owner)
}

/// Verifies that the recovered producer materializes a first-class callable descriptor.
fn require_expr_callable_source<'a>(inst: &'a Instruction, owner: &str) -> Result<&'a Instruction> {
    if inst.op == Op::FirstClassCallableNew {
        Ok(inst)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "{} with non-static callable operand",
            owner
        )))
    }
}
