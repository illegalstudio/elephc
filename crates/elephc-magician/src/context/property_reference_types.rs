//! Purpose:
//! Projects live eval-owned property associations into borrowed reference constraints.
//!
//! Called from:
//! - Interpreter reference publication before replacing a canonical cell's child.
//!
//! Key details:
//! - Property associations retain no receiver and disappear on unset, rebind or object retirement.
//! - Types come from immutable declaration metadata, including mangled parent-private slots.

use super::*;

/// One declaration currently constraining a shared property-reference allocation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EvalPropertyReferenceType {
    pub(crate) owner: String,
    pub(crate) property: String,
    pub(crate) declared: EvalParameterType,
}

impl ElephcEvalContext {
    /// Reads current property associations without extending receiver or reference lifetimes.
    pub(crate) fn property_reference_types(&self, cell: RuntimeCellHandle) -> Vec<EvalPropertyReferenceType> {
        let same_cell = |target: &EvalReferenceTarget| matches!(target,
            EvalReferenceTarget::Cell { cell: source } if source.as_ptr() == cell.as_ptr());
        let mut types = Vec::new();
        for ((identity, storage), target) in &self.dynamic_property_aliases {
            if !same_cell(target) { continue; }
            let Some(class_key) = self.dynamic_objects.get(identity) else { continue; };
            let Some(class) = self.classes.get(class_key) else { continue; };
            let declared = if let Some(private) = storage.strip_prefix('\0') {
                private.split_once('\0').and_then(|(owner, name)| self.class_own_property(owner, name))
            } else { self.class_property(class.name(), storage) };
            let Some((owner, property)) = declared else { continue; };
            let Some(ty) = property.property_type() else { continue; };
            types.push(EvalPropertyReferenceType {
                owner, property: property.name().to_string(), declared: ty.clone(),
            });
        }
        for ((class, name), target) in &self.static_property_aliases {
            if !same_cell(target) { continue; }
            let Some((owner, property)) = self.class_property(class, name) else { continue; };
            let Some(ty) = property.property_type() else { continue; };
            types.push(EvalPropertyReferenceType {
                owner, property: property.name().to_string(), declared: ty.clone(),
            });
        }
        types.sort_by(|left, right| (&left.owner, &left.property).cmp(&(&right.owner, &right.property)));
        types.dedup();
        types
    }
}
