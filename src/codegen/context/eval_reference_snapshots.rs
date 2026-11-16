//! Purpose:
//! Flushes a native reference's current boxed value into an eval scope.
//!
//! Called from:
//! - `FunctionContext::load_eval_local_snapshot_to_mixed` for attached native cells.
//!
//! Key details:
//! - Canonical cells expose their persistent wrapper independently of the raw parameter ABI.
//! - Eval retains that wrapper so reference identity and its actual PHP tag survive the flush.
//! - Managed scope references acquire one stable wrapper; headerless storage keeps its ABI.

use super::*;

impl FunctionContext<'_> {
    /// Shares a managed reference identity while boxing headerless legacy payloads exactly once.
    pub(super) fn own_eval_ref_cell_snapshot_to_mixed(
        &mut self, slot: LocalSlotId, payload_ty: &PhpType,
    ) -> Result<()> {
        let raw = self.next_label("eval_ref_snapshot_raw");
        let done = self.next_label("eval_ref_snapshot_done");
        let owned = self.next_label("eval_ref_snapshot_owned");
        let result = abi::int_result_reg(self.emitter);
        let offset = self.local_offset(slot)?;
        abi::load_at_offset(self.emitter, result, offset);
        abi::emit_call_label(self.emitter, "__rt_reference_cell_canonical");
        abi::emit_branch_if_int_result_nonzero(self.emitter, &owned);
        if self.module.required_runtime_features.eval_scope || self.module.required_runtime_features.eval_bridge {
            if let Some(payload) = crate::types::canonical_property_reference_payload(payload_ty) {
                abi::load_at_offset(self.emitter, result, offset);
                abi::emit_call_label(self.emitter, "__rt_reference_cell_owner");
                abi::emit_branch_if_int_result_zero(self.emitter, &raw);
                abi::emit_call_label(self.emitter,
                    crate::codegen_support::runtime::reference_cells::promotion_entry(payload));
                abi::emit_call_label(self.emitter, "__rt_reference_cell_canonical");
                abi::emit_branch_if_int_result_nonzero(self.emitter, &owned);
            }
        }
        abi::emit_jump(self.emitter, &raw);
        self.emitter.label(&owned);
        abi::emit_call_label(self.emitter, "__rt_incref");
        abi::emit_jump(self.emitter, &done);
        self.emitter.label(&raw);
        self.load_ref_cell_payload_to_result(slot, payload_ty)?;
        self.own_loaded_eval_snapshot_as_mixed(payload_ty);
        self.emitter.label(&done);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{frame, platform::Target, shared_helper::helper_function};
    use crate::ir::{FunctionParam, IrType};

    /// Eval sees the actual canonical child tag, while raw typed parameters retain their ABI fallback.
    #[test]
    fn eval_reference_snapshots_preserve_wrappers_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for ty in [PhpType::Int, PhpType::Float, PhpType::Str, PhpType::Mixed] {
                let target = Target::parse(name).unwrap();
                let mut module = Module::new(target);
                module.required_runtime_features.eval_scope = true;
                let mut function = helper_function("eval_snapshot", PhpType::Void);
                function.params.clear();
                function.params.push(FunctionParam {
                    name: "value".into(), ir_type: IrType::from_php(&ty), php_type: ty.clone(),
                    by_ref: true, variadic: false,
                });
                function.values[0].ir_type = IrType::from_php(&ty);
                function.values[0].php_type = ty.clone();
                let slot = function.add_local(Some("value".into()), IrType::from_php(&ty), ty.clone(), LocalKind::PhpLocal);
                let layout = frame::layout_for_function(&function, target, false, false, false);
                let mut emitter = Emitter::new(target);
                let mut data = DataSection::new();
                let mut shared = SharedCodegenState::default();
                {
                    let mut context = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                        &mut shared, layout, false, false, false, None);
                    context.load_eval_local_snapshot_to_mixed(slot).unwrap();
                }
                let assembly = emitter.output();
                let canonical = assembly.find("__rt_reference_cell_canonical").unwrap();
                let retain = assembly[canonical..].find("__rt_incref").unwrap() + canonical;
                assert!(canonical < retain, "{name} {ty:?}: eval owns the actual reference wrapper");
                assert!(assembly.contains("eval_ref_snapshot_raw"), "{name}: preserve raw parameter layout");
                let promotion = crate::codegen_support::runtime::reference_cells::promotion_entry(
                    crate::types::canonical_property_reference_payload(&ty).unwrap());
                assert!(assembly.contains(promotion), "{name} {ty:?}: managed aliases share their promoted identity");
                assert!(!assembly.contains("__rt_mixed_clone"), "{name}: copying must not break native alias identity");
            }
        }
    }
}
