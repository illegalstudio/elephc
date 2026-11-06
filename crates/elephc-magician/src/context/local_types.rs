//! Purpose:
//! Retains typed-local contracts on persistent reference storage across eval activations.
//!
//! Called from:
//! - Typed declarations, reference promotion, alias binding, and reference write validation.
//!
//! Key details:
//! - Metadata owns no runtime cells; allocation tokens prevent stale contracts after address reuse.

use super::*;
use crate::ffi::array_references::{register_array_reference_cell, ArrayReferenceCellLifetime};

/// Contracts attached to one live reference allocation, independent of its variable names.
pub(super) struct EvalLocalReferenceTypes {
    types: Vec<EvalParameterType>,
    lifetime: ArrayReferenceCellLifetime,
}

impl ElephcEvalContext {
    /// Adds the contracts of a typed alias to shared persistent storage without retaining it.
    pub(crate) fn bind_local_reference_types(&mut self, cell: RuntimeCellHandle, types: &[EvalParameterType]) {
        for address in self.local_reference_retirements.take() { self.local_reference_types.remove(&address); }
        if types.is_empty() { return; }
        let address = cell.as_ptr() as usize;
        let entry = self.local_reference_types.entry(address).or_insert_with(|| EvalLocalReferenceTypes {
            types: Vec::new(), lifetime: register_array_reference_cell(address, &self.local_reference_retirements),
        });
        for ty in types {
            if !entry.types.contains(ty) { entry.types.push(ty.clone()); }
        }
    }

    /// Returns only the contracts belonging to the exact live reference allocation.
    pub(crate) fn local_reference_types(&self, cell: RuntimeCellHandle) -> Vec<EvalParameterType> {
        self.local_reference_types.get(&(cell.as_ptr() as usize)).filter(|entry| entry.lifetime.is_live())
            .map(|entry| entry.types.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Released reference addresses must not impose their contracts on a later allocation.
    #[test]
    fn local_reference_contracts_retire_before_address_reuse() {
        use crate::eval_ir::EvalParameterTypeVariant;
        let allocation = Box::new(0_u8);
        let address = (&*allocation as *const u8) as usize;
        let cell = RuntimeCellHandle::from_raw(address as *mut _);
        let mut context = ElephcEvalContext::new();
        context.bind_local_reference_types(cell, &[EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)]);
        assert_eq!(context.local_reference_types(cell), vec![EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)]);
        crate::ffi::array_references::retire_array_reference_cell(address);
        assert!(context.local_reference_types(cell).is_empty());
        context.bind_local_reference_types(cell, &[EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)]);
        assert_eq!(context.local_reference_types(cell), vec![EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)]);
        crate::ffi::array_references::retire_array_reference_cell(address);
    }
}
