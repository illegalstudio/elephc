//! Purpose:
//! Retires an inherited instance-vtable slot when an override appends optional parameters.
//! The override takes a new, wider slot under its own method key; the old slot keeps its
//! number under a generated key so callers compiled against the ancestor still reach it.
//!
//! Called from:
//! - `crate::types::checker::schema::classes::methods::apply_instance_method()`.
//!
//! Key details:
//! - The retired slot stays at the ancestor's index, so `assert_inherited_vtable_slots_aligned`
//!   finds it through `ClassInfo::narrowed_vtable_slots` rather than the method key.
//! - Codegen fills the retired slot with an EIR adapter that calls the override with only the
//!   ancestor's parameters, letting ordinary call lowering supply the appended defaults.

use crate::types::{FunctionSig, NarrowedVtableSlot};

use super::super::validation::appends_optional_parameters;
use super::state::ClassBuildState;

impl ClassBuildState {
    /// Moves `method_key` to a fresh slot when `child_sig` appends optional parameters to the
    /// inherited `parent_sig`, keeping the inherited slot reachable under a generated key.
    ///
    /// A method without an inherited slot (private, or an instantiated generic) is left alone;
    /// the caller's ordinary slot allocation then assigns the method key its new slot.
    pub(super) fn retire_narrower_vtable_slot(
        &mut self,
        method_key: &str,
        parent_sig: &FunctionSig,
        child_sig: &FunctionSig,
    ) {
        if !appends_optional_parameters(child_sig, parent_sig) {
            return;
        }
        let Some(slot) = self.vtable_slots.remove(method_key) else {
            return;
        };
        let inherited_param_count = parent_sig
            .params
            .iter()
            .filter(|(name, _)| {
                name != crate::func_args::HIDDEN_ARGS_PARAM
                    && name != crate::func_args::HIDDEN_ARGC_PARAM
            })
            .count();
        let key = crate::names::narrowed_vtable_slot_key(method_key, inherited_param_count);
        self.vtable_methods[slot] = key.clone();
        self.vtable_slots.insert(key.clone(), slot);
        self.narrowed_vtable_slots.insert(
            key,
            NarrowedVtableSlot {
                method: method_key.to_string(),
                signature: parent_sig.clone(),
            },
        );
    }
}
