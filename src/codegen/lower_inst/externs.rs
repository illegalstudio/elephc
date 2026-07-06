//! Purpose:
//! Lowers scalar extern function calls from EIR into the target C ABI.
//! Covers the Phase 04 parity path for string, scalar, pointer, and descriptor-backed callable FFI calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Source-order evaluation already happened during AST-to-EIR lowering; this
//!   module only materializes precomputed SSA values into C ABI locations.
//! - String parameters are converted to call-scoped C strings and released
//!   immediately after the foreign call returns.
//! - Callable extern parameters lower to C function pointers. Descriptor-backed
//!   callables use generated trampolines when the callable signature is known.

use crate::codegen::abi;
use crate::codegen::callable_descriptor;
use crate::codegen::platform::Arch;
use crate::codegen_support::DeferredExternCallbackTrampoline;
use crate::ir::{
    ExternDecl, ExternParamDecl, Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId,
};
use crate::names::{function_symbol, php_symbol_key};
use crate::types::{callable_wrapper_sig, FunctionSig, PhpType};

use super::super::context::FunctionContext;
use super::{
    emit_runtime_callable_invoker_inline, expect_data, expect_operand, load_value_to_first_int_arg,
    store_if_result,
};
use crate::codegen::{CodegenIrError, Result};

/// Lowers an EIR extern call to a platform-mangled C symbol call.
pub(super) fn lower_extern_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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

    let c_param_types = decl.params.iter().map(c_abi_param_type).collect::<Vec<_>>();
    let string_arg_count = decl
        .params
        .iter()
        .filter(|param| param.php_type.codegen_repr() == PhpType::Str)
        .count();
    let cleanup_bytes = string_arg_count * 16;
    if cleanup_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
    }
    let cleanup_base_reg = abi::temp_int_reg(ctx.emitter.target);
    let mut cleanup_idx = 0usize;
    let mut pushed_arg_bytes = 0usize;
    for (idx, param) in decl.params.iter().enumerate() {
        let value = expect_operand(inst, idx)?;
        let pushed_ty = materialize_extern_arg(ctx, value, param)?;
        if param.php_type.codegen_repr() == PhpType::Str {
            abi::emit_temporary_stack_address(ctx.emitter, cleanup_base_reg, pushed_arg_bytes);
            abi::emit_store_to_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                cleanup_base_reg,
                cleanup_idx * 16,
            );
            cleanup_idx += 1;
        }
        abi::emit_push_result_value(ctx.emitter, &pushed_ty);
        pushed_arg_bytes += temp_slot_size(&pushed_ty);
    }

    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &c_param_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let symbol = ctx.emitter.target.extern_symbol(&decl.name);
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    normalize_extern_return(ctx, &decl.return_php_type)?;
    release_borrowed_cstr_temps(ctx, string_arg_count, cleanup_bytes, &decl.return_php_type);
    store_if_result(ctx, inst)
}

/// Returns the extern declaration addressed by the instruction's function-name immediate.
fn extern_decl<'a>(ctx: &'a FunctionContext<'_>, inst: &Instruction) -> Result<&'a ExternDecl> {
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
    match (position, ty.codegen_repr()) {
        ("parameter", PhpType::Callable) => Ok(()),
        (
            _,
            PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Pointer(_),
        ) => Ok(()),
        (_, other) => Err(CodegenIrError::unsupported(format!(
            "extern {} {} type {:?}",
            name, position, other
        ))),
    }
}

/// Returns the C ABI type for an extern parameter after PHP-specific conversion.
fn c_abi_param_type(param: &ExternParamDecl) -> PhpType {
    match param.php_type.codegen_repr() {
        PhpType::Callable => PhpType::Pointer(None),
        PhpType::Str => PhpType::Pointer(None),
        other => other,
    }
}

/// Returns the temporary stack bytes used by one pre-materialized extern argument.
fn temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

/// Loads and coerces an SSA value into the ABI result register expected by an extern parameter.
fn materialize_extern_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param: &ExternParamDecl,
) -> Result<PhpType> {
    let target_ty = param.php_type.codegen_repr();
    let actual_ty = ctx.value_php_type(value)?;
    match (&target_ty, actual_ty.codegen_repr()) {
        (PhpType::Callable, PhpType::Callable) => {
            materialize_extern_callable_arg(ctx, value);
            return Ok(PhpType::Pointer(None));
        }
        (PhpType::Callable, PhpType::Str) => {
            materialize_extern_string_callable_arg(ctx, value)?;
            return Ok(PhpType::Pointer(None));
        }
        (PhpType::Pointer(_), PhpType::Void) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        (PhpType::Str, PhpType::Str) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
            return Ok(PhpType::Pointer(None));
        }
        (
            PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str,
            PhpType::Mixed | PhpType::Union(_),
        ) => {
            materialize_mixed_extern_arg(ctx, value, &target_ty)?;
            if target_ty == PhpType::Str {
                return Ok(PhpType::Pointer(None));
            }
        }
        (PhpType::Int | PhpType::Bool, PhpType::TaggedScalar) => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        }
        (PhpType::Float, PhpType::TaggedScalar) => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            abi::emit_int_result_to_float_result(ctx.emitter);
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
                param.name, expected, actual
            )))
        }
    }
    Ok(target_ty)
}

/// Casts a boxed Mixed/Union value into the concrete scalar shape an extern parameter expects.
fn materialize_mixed_extern_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    target_ty: &PhpType,
) -> Result<()> {
    load_value_to_first_int_arg(ctx, value)?;
    match target_ty {
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        PhpType::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "extern Mixed argument cast to PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Materializes a constant string callback as a direct C function pointer.
fn materialize_extern_string_callable_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let callback_name = const_string_value(ctx, value).ok_or_else(|| {
        CodegenIrError::unsupported("extern callable parameter from runtime string value")
    })?;
    let Some((function_name, signature)) = ctx
        .callable_function_by_name(callback_name)
        .filter(|function| !function.flags.is_main)
        .map(|function| {
            (
                function.name.clone(),
                callable_wrapper_sig(&super::function_signature_from_eir(function)),
            )
        })
    else {
        abi::emit_symbol_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            &function_symbol(callback_name),
        );
        return Ok(());
    };
    materialize_extern_static_function_descriptor_callback(ctx, &function_name, &signature)?;
    Ok(())
}

/// Materializes a resolved static user function through the descriptor-backed extern trampoline.
fn materialize_extern_static_function_descriptor_callback(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
    signature: &FunctionSig,
) -> Result<()> {
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &signature, &[]);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &function_symbol(function_name),
        Some(function_name),
        callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
        Some(&signature),
        &[],
        &[],
        callable_descriptor::CallableDescriptorInvocation::named(
            callable_descriptor::CallableDescriptorShape::Function,
            function_name,
        ),
        Some(&invoker_label),
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &descriptor_label,
    );
    emit_stateful_extern_callback_trampoline(ctx, &signature);
    Ok(())
}

/// Returns the literal string for a constant-string value.
fn const_string_value<'a>(ctx: &'a FunctionContext<'_>, value: ValueId) -> Option<&'a str> {
    let value = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if !matches!(inst.op, Op::ConstStr) {
        return None;
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Materializes an EIR callable descriptor as a C-compatible callback function pointer.
fn materialize_extern_callable_arg(ctx: &mut FunctionContext<'_>, value: ValueId) {
    let callback_sig = callable_signature_for_value(ctx, value);
    ctx.load_value_to_result(value)
        .expect("callable extern arg value was validated before materialization");
    if let Some(callback_sig) = callback_sig {
        emit_stateful_extern_callback_trampoline(ctx, &callback_sig);
        return;
    }
    callable_descriptor::emit_load_entry_from_descriptor(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        abi::int_result_reg(ctx.emitter),
    );
}

/// Emits a descriptor-backed extern callback trampoline and leaves its address in the result register.
fn emit_stateful_extern_callback_trampoline(
    ctx: &mut FunctionContext<'_>,
    callback_sig: &FunctionSig,
) {
    let slot_name = ctx.next_label("extern_callback_descriptor");
    let slot_label = ctx.data.add_comm(slot_name, 8);
    let trampoline_label = ctx.next_label("extern_callback_trampoline");
    let done_label = ctx.next_label("extern_callback_trampoline_done");
    let trampoline = DeferredExternCallbackTrampoline {
        label: trampoline_label.clone(),
        descriptor_slot_label: slot_label.clone(),
        visible_arg_types: extern_callback_visible_arg_types(callback_sig),
        return_type: callback_sig.return_type.codegen_repr(),
    };

    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_extern_callback_trampoline(ctx.emitter, &trampoline);
    ctx.emitter.label(&done_label);

    ctx.emitter
        .comment("extern callback: bind descriptor trampoline");
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &slot_label,
        0,
    );
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &slot_label,
        0,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &trampoline_label,
    );
}

/// Returns the C-visible extern callback parameter types used before PHP boxing.
fn extern_callback_visible_arg_types(callback_sig: &FunctionSig) -> Vec<PhpType> {
    callback_sig
        .params
        .iter()
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Mixed | PhpType::Union(_) => PhpType::Int,
            concrete => concrete,
        })
        .collect()
}

/// Recovers a callable signature from a value definition when the EIR carries enough metadata.
fn callable_signature_for_value(ctx: &FunctionContext<'_>, value: ValueId) -> Option<FunctionSig> {
    callable_signature_for_value_seen(ctx, value, &mut Vec::new())
}

/// Recovers callable signatures while avoiding cycles through locals or forwarding opcodes.
fn callable_signature_for_value_seen(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    seen: &mut Vec<ValueId>,
) -> Option<FunctionSig> {
    if seen.contains(&value) {
        return None;
    }
    seen.push(value);
    let signature = value_instruction(ctx, value).and_then(|inst| match inst.op {
        Op::FirstClassCallableNew => first_class_callable_signature(ctx, inst),
        Op::LoadLocal => load_local_callable_signature(ctx, inst, seen),
        Op::Acquire | Op::Move | Op::Borrow => inst
            .operands
            .first()
            .and_then(|operand| callable_signature_for_value_seen(ctx, *operand, seen)),
        _ => None,
    });
    seen.pop();
    signature
}

/// Returns the instruction that defines an SSA value.
fn value_instruction<'a>(ctx: &'a FunctionContext<'_>, value: ValueId) -> Option<&'a Instruction> {
    let value = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    ctx.function.instruction(inst)
}

/// Recovers a callable signature from all stores feeding one local slot.
fn load_local_callable_signature(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    seen: &mut Vec<ValueId>,
) -> Option<FunctionSig> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return None;
    };
    stored_callable_signature_for_slot(ctx, slot, seen)
}

/// Returns a slot signature only when every callable store agrees on one contract.
fn stored_callable_signature_for_slot(
    ctx: &FunctionContext<'_>,
    slot: LocalSlotId,
    seen: &mut Vec<ValueId>,
) -> Option<FunctionSig> {
    let mut signature = None;
    for inst in &ctx.function.instructions {
        if !matches!(inst.op, Op::StoreLocal) || inst.immediate != Some(Immediate::LocalSlot(slot))
        {
            continue;
        }
        let value = *inst.operands.first()?;
        let candidate = callable_signature_for_value_seen(ctx, value, seen)?;
        if signature
            .as_ref()
            .is_some_and(|existing| existing != &candidate)
        {
            return None;
        }
        signature = Some(candidate);
    }
    signature
}

/// Recovers the callable signature for a first-class callable descriptor.
fn first_class_callable_signature(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Option<FunctionSig> {
    let target = first_class_callable_target(ctx, inst)?;
    if let Some((receiver_label, method_name)) = target.rsplit_once("::") {
        return first_class_method_callable_signature(ctx, inst, receiver_label, method_name);
    }
    ctx.callable_function_by_name(target)
        .map(super::function_signature_from_eir)
}

/// Returns the first-class callable target string attached to an EIR instruction.
fn first_class_callable_target<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Recovers a first-class method callable signature from receiver metadata.
fn first_class_method_callable_signature(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    receiver_label: &str,
    method_name: &str,
) -> Option<FunctionSig> {
    let method_key = php_symbol_key(method_name);
    if receiver_label.trim_start_matches('\\') == "object" {
        let receiver = inst.operands.first().copied()?;
        let PhpType::Object(class_name) = ctx.value_php_type(receiver).ok()?.codegen_repr() else {
            return None;
        };
        return ctx
            .module
            .class_infos
            .get(class_name.trim_start_matches('\\'))
            .and_then(|class| class.methods.get(&method_key).cloned());
    }
    let receiver = receiver_label.trim_start_matches('\\');
    ctx.module
        .class_infos
        .get(receiver)
        .and_then(|class| class.static_methods.get(&method_key))
        .cloned()
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
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_cstr_to_str");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "extern return type {:?}",
            other
        ))),
    }
}

/// Releases call-scoped C-string argument copies after preserving the extern return value.
fn release_borrowed_cstr_temps(
    ctx: &mut FunctionContext<'_>,
    string_arg_count: usize,
    cleanup_bytes: usize,
    return_ty: &PhpType,
) {
    if string_arg_count == 0 {
        return;
    }
    let saved_return_bytes = push_ffi_return_value(ctx, return_ty);
    for idx in 0..string_arg_count {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            saved_return_bytes + idx * 16,
        );
        abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    }
    pop_ffi_return_value(ctx, return_ty);
    abi::emit_release_temporary_stack(ctx.emitter, cleanup_bytes);
}

/// Pushes the current extern return value while borrowed C-string args are freed.
fn push_ffi_return_value(ctx: &mut FunctionContext<'_>, return_ty: &PhpType) -> usize {
    match return_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => 0,
        PhpType::Float => {
            abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
            16
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
            16
        }
        _ => {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            16
        }
    }
}

/// Restores a return value preserved by `push_ffi_return_value`.
fn pop_ffi_return_value(ctx: &mut FunctionContext<'_>, return_ty: &PhpType) {
    match return_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => {}
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        _ => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}

/// Sign-extends a C `int` return into the target integer result register.
fn emit_sign_extend_i32_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sxtw x0, w0"); // sign-extend the C int return into PHP's 64-bit integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movsxd rax, eax"); // sign-extend the C int return into PHP's 64-bit integer result
        }
    }
}
