//! Purpose:
//! Stores conditional array origins on property bindings independently from value allocations.
//!
//! Called from:
//! - Successful property initialization, writes and live reference constraint projection.
//!
//! Key details:
//! - Instance keys use physical storage names and object identities retired by native finalizers.
//! - Contracts own no runtime cells; static origins belong to their declaring class slot.

use super::*;
use crate::ffi::array_references::{ArrayReferenceCellLifetime, register_array_reference_cell};

/// A binding's conditional declaration and weak currently published array allocation.
#[derive(Clone)]
pub(super) struct EvalPropertyArrayOrigin {
    pub(super) ty: EvalParameterType,
    source: Option<(RuntimeCellHandle, ArrayReferenceCellLifetime)>,
}

impl ElephcEvalContext {
    /// Borrows the content obligation of one live physical instance binding.
    pub(crate) fn dynamic_property_array_type(&self, identity: u64, storage: &str) -> Option<&EvalParameterType> {
        self.dynamic_property_array_types.get(&(identity, storage.to_string())).map(|origin| &origin.ty)
    }

    /// Publishes one successful instance initializer without extending its receiver lifetime.
    pub(crate) fn set_dynamic_property_array_type(&mut self, identity: u64, storage: &str, ty: EvalParameterType) {
        self.dynamic_property_array_types.insert((identity, storage.to_string()), EvalPropertyArrayOrigin { ty, source: None });
    }

    /// Borrows a static binding's origin using the same class normalization as static values.
    pub(crate) fn static_property_array_type(&self, class: &str, property: &str) -> Option<&EvalParameterType> {
        self.static_property_array_types.get(&(normalize_class_name(class), property.to_string())).map(|origin| &origin.ty)
    }

    /// Publishes one successful static initializer independently from later value replacements.
    pub(crate) fn set_static_property_array_type(&mut self, class: &str, property: &str, ty: EvalParameterType) {
        self.static_property_array_types.insert((normalize_class_name(class), property.to_string()), EvalPropertyArrayOrigin { ty, source: None });
    }
}

impl ElephcEvalContext {
    /// Retargets only a live instance binding's weak array source before displaced-value callbacks.
    pub(crate) fn publish_dynamic_property_array_source(&mut self, identity: u64, storage: &str, value: Option<RuntimeCellHandle>) {
        let Some(origin) = self.dynamic_property_array_types.get_mut(&(identity, storage.to_string())) else { return; };
        origin.source = value.map(|value| (value.borrowed(), register_array_reference_cell(value.as_ptr() as usize, &self.array_reference_retirements)));
    }

    /// Retargets one declaring static slot without retaining the published PHP value.
    pub(crate) fn publish_static_property_array_source(&mut self, class: &str, property: &str, value: Option<RuntimeCellHandle>) {
        let Some(origin) = self.static_property_array_types.get_mut(&(normalize_class_name(class), property.to_string())) else { return; };
        origin.source = value.map(|value| (value.borrowed(), register_array_reference_cell(value.as_ptr() as usize, &self.array_reference_retirements)));
    }

    /// Snapshots property-owned roots only while both the binding and its exact allocation remain live.
    pub(super) fn property_array_reference_sources(&self) -> Vec<super::array_reference_types::EvalArrayReferenceSource> {
        self.dynamic_property_array_types.values().chain(self.static_property_array_types.values())
            .filter_map(|origin| origin.source.as_ref().filter(|(_, lifetime)| lifetime.is_live())
                .map(|(array, _)| super::array_reference_types::EvalArrayReferenceSource { array: *array, contracts: vec![origin.ty.clone()] }))
            .collect()
    }
}

impl ElephcEvalContext {
    /// Refines live pending property bindings after their canonical reference successfully changes.
    pub(crate) fn refine_property_array_reference_types(&mut self, cell: RuntimeCellHandle, ty: &EvalParameterType) {
        let pending = |origin: &EvalPropertyArrayOrigin| crate::eval_ir::pending_array_contents(&crate::eval_ir::generic_type(&origin.ty));
        for (key, target) in &self.dynamic_property_aliases {
            if matches!(target, EvalReferenceTarget::Cell { cell: source } if source.as_ptr() == cell.as_ptr()) {
                if let Some(origin) = self.dynamic_property_array_types.get_mut(key).filter(|origin| pending(origin)) {
                    origin.ty = ty.clone();
                }
            }
        }
        for (key, target) in &self.static_property_aliases {
            if matches!(target, EvalReferenceTarget::Cell { cell: source } if source.as_ptr() == cell.as_ptr()) {
                if let Some(origin) = self.static_property_array_types.get_mut(key).filter(|origin| pending(origin)) {
                    origin.ty = ty.clone();
                }
            }
        }
    }
}
