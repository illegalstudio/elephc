//! Purpose:
//! Publishes live native object binding contracts before interpreted scope writes.
//!
//! Called from:
//! - Native local snapshot flushing around eval boundaries.
//!
//! Key details:
//! - A future object slot with a zero occupant does not establish a local binding contract.
//! - Existing object and reference occupants retain their assignment-time validation.

use super::*;

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
