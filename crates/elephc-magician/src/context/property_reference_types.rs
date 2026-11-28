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

/// A stack-owned prospective declaration for one canonical reference publication.
pub(super) struct PropertyReferenceAdmission {
    reference: usize,
    declaration: EvalPropertyReferenceType,
    claimed: bool,
}

impl ElephcEvalContext {
    /// Resolves immutable declaration metadata from an eval object's physical private or visible slot.
    pub(crate) fn dynamic_storage_property(&self, identity: u64, storage: &str) -> Option<(String, EvalClassProperty)> {
        let class = self.dynamic_object_class(identity)?;
        if let Some(private) = storage.strip_prefix('\0') {
            let (owner, name) = private.split_once('\0')?;
            self.class_own_property(owner, name)
        } else { self.class_property(class.name(), storage) }
    }

    /// Reads current property associations without extending receiver or reference lifetimes.
    pub(crate) fn property_reference_types(&self, cell: RuntimeCellHandle) -> Vec<EvalPropertyReferenceType> {
        let same_cell = |target: &EvalReferenceTarget| matches!(target,
            EvalReferenceTarget::Cell { cell: source } if source.as_ptr() == cell.as_ptr());
        let mut types = Vec::new();
        for ((identity, storage), target) in &self.dynamic_property_aliases {
            if !same_cell(target) { continue; }
            let Some((owner, property)) = self.dynamic_storage_property(*identity, storage) else { continue; };
            for ty in property.property_type().into_iter().chain(self.dynamic_property_array_type(*identity, storage)) {
                types.push(EvalPropertyReferenceType {
                    owner: owner.clone(), property: property.name().to_string(), declared: ty.clone(),
                });
            }
        }
        for ((class, name), target) in &self.static_property_aliases {
            if !same_cell(target) { continue; }
            let Some((owner, property)) = self.class_property(class, name) else { continue; };
            for ty in property.property_type().into_iter().chain(self.static_property_array_type(&owner, name)) {
                types.push(EvalPropertyReferenceType {
                    owner: owner.clone(), property: property.name().to_string(), declared: ty.clone(),
                });
            }
        }
        types.sort_by(|left, right| (&left.owner, &left.property).cmp(&(&right.owner, &right.property)));
        types
    }
}

impl ElephcEvalContext {
    /// Adds one prospective source to the next publication while keeping callback reads on actual storage.
    pub(crate) fn with_property_reference_admission<T>(&mut self,reference: RuntimeCellHandle,
        owner: &str,property: &str,declared: &EvalParameterType,body: impl FnOnce(&mut Self)->T,
    ) -> T {
        let depth=self.property_reference_admissions.len();
        self.property_reference_admissions.push(PropertyReferenceAdmission {
            reference:reference.as_ptr() as usize,
            declaration:EvalPropertyReferenceType {owner:owner.into(),property:property.into(),declared:declared.clone()},claimed:false,
        });
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||body(self)));
        self.property_reference_admissions.truncate(depth);
        match result {Ok(result)=>result,Err(panic)=>std::panic::resume_unwind(panic)}
    }

    /// Copies the next prospective declaration once, leaving nested callback publications unconstrained by it.
    pub(crate) fn claim_property_reference_admission(&mut self,reference: RuntimeCellHandle)
        -> Option<EvalPropertyReferenceType> {
        let source=self.property_reference_admissions.iter_mut().rev()
            .find(|source|!source.claimed && source.reference==reference.as_ptr() as usize)?;
        source.claimed=true;
        Some(source.declaration.clone())
    }
}
