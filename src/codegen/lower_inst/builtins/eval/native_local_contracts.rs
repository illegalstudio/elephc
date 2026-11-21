//! Purpose:
//! Publishes native storage constraints and checked source evidence before eval snapshots.
//!
//! Called from:
//! - Native local snapshot flushing around eval boundaries.
//!
//! Key details:
//! - A future object slot with a zero occupant does not establish a local binding contract.
//! - Existing object and reference occupants retain their assignment-time validation.
//! - Source roles retain full logical terms without enabling inferred local-kind constraints.

use super::*;

/// Publishes an authoritative source contract without inferring it from the current frame shape.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_scope_bind_local_type(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let Some(Immediate::EvalScopeStore(metadata)) = &inst.immediate else {
        return Err(CodegenIrError::invalid_module("eval binding type requires checked metadata"));
    };
    let source = metadata.role != crate::ir::EvalBindingRole::Contract;
    let contract = match &metadata.contract {
        Some(ty) => (if source { crate::types::native_type_spec::logical_type_spec(ty, false) }
            else { eval_native_php_type_spec(ty, false) })
            .ok_or_else(|| CodegenIrError::invalid_module("eval binding type has no native contract spelling"))?,
        None => "mixed".to_string(),
    };
    let name = super::scope_access::eval_scope_instruction_name(ctx, inst)?;
    let (name, name_len) = ctx.data.add_string(name.as_bytes());
    let (ty, ty_len) = ctx.data.add_string(contract.as_bytes());
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_scope(ctx)?;
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &name);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), name_len as i64);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), &ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), ty_len as i64);
    let setter = if source {
        use elephc_semantics::native_local_contract::{SOURCE_DECLARED, SOURCE_ARRAY_CONTENTS};
        let declared = if metadata.role == crate::ir::EvalBindingRole::DeclaredSource { SOURCE_DECLARED } else { 0 };
        let contents = if ctx.module.semantic_profile.contains(elephc_semantics::StrictFeature::ArrayTypes) {
            SOURCE_ARRAY_CONTENTS
        } else { 0 };
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 5),
            (declared | contents) as i64);
        "__elephc_eval_scope_source_type_v1"
    } else { "__elephc_eval_scope_binding_type_v1" };
    let symbol = ctx.emitter.target.extern_symbol(setter);
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    if inst.op == Op::EvalScopeStartBinding {
        let descriptor = metadata.contract.as_ref().and_then(
            crate::semantics::conversions::inferred_binding_contract).map(|contract|
                crate::codegen::runtime_metadata::local_reference_descriptor(ctx.module, &contract))
            .transpose()?;
        load_eval_scope_to_arg(ctx, 0);
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &name);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), name_len as i64);
        let argument = abi::int_arg_reg_name(ctx.emitter.target, 3);
        if let Some(descriptor) = descriptor {
            abi::emit_symbol_address(ctx.emitter, argument, &descriptor);
        } else { abi::emit_load_int_immediate(ctx.emitter, argument, 0); }
        let symbol = ctx.emitter.target.extern_symbol("__elephc_reference_scope_binding_v1");
        abi::emit_call_label(ctx.emitter, &symbol);
    }
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Selects a current object contract without pinning an uninitialized future frame slot.
pub(super) fn emit_eval_native_local_type(
    ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal,
) -> Result<()> {
    let PhpType::Object(class) = &local.ty else {
        emit_contract(ctx, local, "");
        return Ok(());
    };
    let unbound = ctx.next_label("eval_native_contract_unbound");
    let done = ctx.next_label("eval_native_contract_done");
    ctx.load_local_to_result(local.slot)?;
    abi::emit_branch_if_int_result_zero(ctx.emitter, &unbound);
    emit_contract(ctx, local, if class.is_empty() { "object" } else { class });
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&unbound);
    emit_contract(ctx, local, "");
    ctx.emitter.label(&done);
    Ok(())
}

/// Installs or clears the snapshot's native contract using the shared eval scope ABI.
fn emit_contract(ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal, expected: &str) {
    let (name, name_len) = ctx.data.add_string(local.name.as_bytes());
    let (ty, ty_len) = ctx.data.add_string(expected.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &name);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), name_len as i64);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), &ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), ty_len as i64);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_native_type_v1");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{data_section::DataSection, emit::Emitter, platform::Target,
        shared_state::SharedCodegenState};

    /// Every target selects an absent contract before publishing a future object-slot class.
    #[test]
    fn native_eval_contracts_check_object_binding_presence_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            for ty in [PhpType::Object("TypeError".into()), PhpType::Mixed] {
                let mut function = crate::codegen::shared_helper::helper_function("scope_contract", PhpType::Void);
                let slot = function.add_local(Some("error".into()), crate::ir::IrType::from_php(&ty),
                    ty.clone(), LocalKind::PhpLocal);
                let local = EvalSyncLocal { name: "error".into(), slot, ty: ty.clone() };
                let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
                let mut emitter = Emitter::new(target);
                let mut data = DataSection::new();
                let mut shared = SharedCodegenState::default();
                {
                    let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                        &mut shared, layout, false, false, false, None);
                    emit_eval_native_local_type(&mut ctx, &local).unwrap();
                }
                let asm = emitter.output();
                let setter = "__elephc_eval_scope_native_type_v1";
                if matches!(ty, PhpType::Object(_)) {
                    let presence = asm.find("eval_native_contract_unbound").unwrap();
                    assert!(presence < asm.find(setter).unwrap(), "{name}: {asm}");
                    assert_eq!(asm.matches(setter).count(), 2, "{name}: {asm}");
                } else {
                    assert!(!asm.contains("eval_native_contract_unbound"), "{name}: {asm}");
                    assert_eq!(asm.matches(setter).count(), 1, "{name}: {asm}");
                }
            }
        }
    }
}

/// Ends one private binding while containing destructor exceptions until Rust has returned.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_scope_end_binding(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let name = super::scope_access::eval_scope_instruction_name(ctx, inst)?;
    let (name, length) = ctx.data.add_string(name.as_bytes());
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_scope(ctx)?;
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &name);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), length as i64);
    abi::emit_call_label(ctx.emitter, "__rt_eval_scope_end_binding");
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}
