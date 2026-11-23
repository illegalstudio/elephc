//! Purpose:
//! Resolves the caller signature of an instance-vtable slot retired by a widening override.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_narrowed_vtable_adapters()` and the class-vtable
//!   emission in `crate::codegen_support::runtime::data::user`.
//!
//! Key details:
//! - The checker snapshots the ancestor's signature when it retires the slot, but later passes
//!   still refine untyped parameter storage. The adapter must receive exactly what the ancestor's
//!   call sites pass, so the ancestor that still names the slot by the method key wins.
//! - The snapshot is the fallback when reachability pruned every such ancestor.

use std::collections::HashMap;

use super::ClassInfo;
use crate::types::FunctionSig;

impl ClassInfo {
    /// Returns the signature callers compile against for the retired slot `slot_key`.
    ///
    /// Walks the parent chain for the nearest ancestor whose method key still owns the slot's
    /// number and returns that ancestor's final method signature. Returns `None` when
    /// `slot_key` is not a retired slot of this class.
    pub fn narrowed_slot_signature<'a>(
        &'a self,
        slot_key: &str,
        classes: &'a HashMap<String, ClassInfo>,
    ) -> Option<&'a FunctionSig> {
        let narrowed = self.narrowed_vtable_slots.get(slot_key)?;
        let slot = self.vtable_slots.get(slot_key).copied();
        let mut seen = std::collections::HashSet::new();
        let mut current = self.parent.as_deref();
        while let Some(name) = current {
            let name = name.trim_start_matches('\\');
            if !seen.insert(name) {
                break;
            }
            let Some(ancestor) = classes.get(name) else { break; };
            if slot.is_some() && ancestor.vtable_slots.get(&narrowed.method).copied() == slot {
                if let Some(signature) = ancestor.methods.get(&narrowed.method) {
                    return Some(signature);
                }
            }
            current = ancestor.parent.as_deref();
        }
        Some(&narrowed.signature)
    }
}
