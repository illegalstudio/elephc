//! Purpose:
//! Invalidates eval array-reference metadata when its boxed runtime cell is freed.
//!
//! Called from:
//! - Context array-alias registration and the generated heap-free callback.
//!
//! Key details:
//! - The registry owns no cells or contexts. Shared validity tokens distinguish address reuse.
//! - Retirement changes validity flags and queues removals, never mutating a borrowed eval context.

pub(crate) use elephc_memoir::cell_retirement::{
    CellLifetime as ArrayReferenceCellLifetime,
    CellRetirements as ArrayReferenceRetirements,
    observe as register_array_reference_cell,
};
#[cfg(test)]
use elephc_memoir::cell_retirement::retire as retire_array_reference_cell;

/// Receives a validated dying Mixed cell address without letting a panic cross the C ABI.
#[cfg(not(test))]
pub(crate) extern "C" fn retire_array_reference_cell_callback(cell: usize) {
    let _ = std::panic::catch_unwind(|| elephc_memoir::cell_retirement::retire(cell));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ElephcEvalContext, EvalArrayReferenceKey, EvalReferenceTarget};
    use crate::value::RuntimeCellHandle;

    /// Reusing one address creates a new allocation token and never revives old reference targets.
    #[test]
    fn retired_array_cell_cannot_inherit_reference_metadata() {
        let allocation = Box::new(0_u8);
        let address = (&*allocation as *const u8) as usize;
        let cell = RuntimeCellHandle::from_raw(address as *mut _);
        let key = EvalArrayReferenceKey::Int(0);
        let mut context = ElephcEvalContext::new();
        context.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        assert!(context.array_element_alias(cell, &key).is_some());
        retire_array_reference_cell(address);
        assert!(context.array_element_alias(cell, &key).is_none());
        let retirements = ArrayReferenceRetirements::default();
        let fresh_lifetime = register_array_reference_cell(address, &retirements);
        assert!(fresh_lifetime.is_live());
        assert!(context.array_element_alias(cell, &key).is_none());
        context.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        assert!(context.array_element_alias(cell, &key).is_some());
        retire_array_reference_cell(address);
        assert!(!fresh_lifetime.is_live());
        assert_eq!(retirements.take(), vec![address]);
        assert!(context.array_element_alias(cell, &key).is_none());
    }

    /// Independent contexts observe the same retirement without a callback mutating either context.
    #[test]
    fn array_cell_retirement_invalidates_all_contexts() {
        let allocation = Box::new(0_u8);
        let address = (&*allocation as *const u8) as usize;
        let cell = RuntimeCellHandle::from_raw(address as *mut _);
        let key = EvalArrayReferenceKey::Int(0);
        let mut first = ElephcEvalContext::new();
        let mut second = ElephcEvalContext::new();
        first.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        second.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        retire_array_reference_cell(address);
        assert!(first.array_element_alias(cell, &key).is_none());
        assert!(second.array_element_alias(cell, &key).is_none());
    }
}
