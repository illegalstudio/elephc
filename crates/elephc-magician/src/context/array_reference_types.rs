//! Purpose:
//! Associates inferred array contents with live array-owned reference element sources.
//!
//! Called from:
//! - Successful array publication and canonical reference candidate validation.
//!
//! Key details:
//! - Array allocation tokens own constraint lifetimes, not the scalar referent's lifetime.
//! - Returned snapshots borrow no maps and retain no PHP values.

use super::*;
use crate::eval_ir::EvalParameterType;

/// An array-owned element association inspected before a shared referent can change.
pub(crate) struct EvalArrayReferenceSource {
    pub(crate) array: RuntimeCellHandle,
    pub(crate) contracts: Vec<EvalParameterType>,
}

impl ElephcEvalContext {
    /// Replaces one published array's content obligations without retaining its runtime allocation.
    pub(crate) fn set_array_reference_contracts(&mut self, array: RuntimeCellHandle, contracts: Vec<EvalParameterType>) {
        self.prune_retired_array_aliases();
        let address = array.as_ptr() as usize;
        if contracts.is_empty() && !self.array_element_aliases.contains_key(&address) { return; }
        let aliases = self.array_element_aliases.entry(address).or_insert_with(|| {
            let lifetime = crate::ffi::array_references::register_array_reference_cell(address, &self.array_reference_retirements);
            EvalArrayReferenceAliases { targets: std::collections::HashMap::new(), contracts: Vec::new(), lifetime }
        });
        aliases.contracts = contracts;
    }

    /// Snapshots live root contracts so nested references retain their original owner lifetime.
    pub(crate) fn array_reference_sources(&self) -> Vec<EvalArrayReferenceSource> {
        self.array_element_aliases.iter().filter(|(_, aliases)| aliases.lifetime.is_live() && !aliases.contracts.is_empty())
            .map(|(address, aliases)| EvalArrayReferenceSource {
                array: RuntimeCellHandle::from_raw(*address as *mut _).borrowed(), contracts: aliases.contracts.clone(),
            }).chain(self.property_array_reference_sources()).collect()
    }
}
