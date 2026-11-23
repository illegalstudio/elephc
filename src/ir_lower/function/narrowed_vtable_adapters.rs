//! Purpose:
//! Builds the EIR adapters behind instance-vtable slots retired by widening overrides.
//! A caller compiled against an ancestor passes the ancestor's arguments; the adapter forwards
//! exactly those to the override, so ordinary call lowering supplies its appended defaults.
//!
//! Called from:
//! - `crate::ir_lower::program` after the interface adapters.
//!
//! Key details:
//! - One adapter per (implementing class, retired slot), shared by every class inheriting it.
//! - The caller signature is the ancestor's final one (`ClassInfo::narrowed_slot_signature`).
//! - The forwarded call dispatches through the owner's wider slot, so a later override that
//!   inherits the retired slot without redeclaring the method still runs its own body.
//! - Forwarding only the passed arguments keeps `func_num_args()` equal to the caller's count.
//! - A retired signature carrying generated `func_args` slots also gets a source-ABI adapter,
//!   published in the source-visible vtable, whose caller passes no generated slots.

use std::collections::BTreeMap;

use super::interface_adapters::lower_interface_adapter;
use super::*;

/// Adds one EIR adapter for every implemented retired slot of every class in the module.
pub(crate) fn lower_narrowed_vtable_adapters(module: &mut Module) {
    let mut adapters = BTreeMap::new();
    for class in module.class_infos.values() {
        for (key, slot) in &class.narrowed_vtable_slots {
            let Some(owner) = class.method_impl_classes.get(&slot.method) else { continue; };
            let Some(caller) = class.narrowed_slot_signature(key, &module.class_infos) else { continue; };
            let name = crate::names::narrowed_vtable_adapter_name(owner, key, false);
            adapters.insert(name, (owner.clone(), slot.method.clone(), caller.clone()));
            if !crate::codegen_support::source_method_adapters::uses_physical_func_args_abi(caller) {
                continue;
            }
            // Variadic signatures never retire a slot, so the projection cannot fail here.
            let Ok(source) =
                crate::codegen_support::source_method_adapters::source_visible_signature(caller)
            else { continue; };
            let name = crate::names::narrowed_vtable_adapter_name(owner, key, true);
            adapters.insert(name, (owner.clone(), slot.method.clone(), source));
        }
    }
    for (name, (owner, method, caller)) in adapters {
        let function = lower_interface_adapter(module, &name, &owner, &method, &caller);
        module.add_function(function);
    }
}
