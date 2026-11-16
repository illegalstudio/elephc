//! Purpose:
//! Borrows materialized eval scope owners for the native suspended-frame collector.
//!
//! Called from:
//! - Generated native frame owner visitors for their owned eval scope handles.
//!
//! Key details:
//! - Only the slots scope cleanup actually owns participate; aliases remain borrowed.
//! - Exact pointer-field addresses preserve owner identity across recursive collector walks.
//! - The callback performs only collector bookkeeping, with no PHP or ownership changes.

use crate::abi::ElephcEvalScope;
use crate::value::RuntimeCell;
use std::ffi::c_void;

/// Pure native slot visitor, with the collector's ordinary tag-zero owner descriptor.
type OwnerVisitor = unsafe extern "C" fn(*const *mut RuntimeCell, u64, *mut c_void);

/// Enumerates actual owned eval cells as saved-frame edges rather than apparent external roots.
///
/// # Safety
/// The scope is null or live and suspended, with immobile entries until this call returns.
/// The callback cannot invoke PHP, unwind, mutate or free the scope, or retain slot addresses.
/// Its context is borrowed unchanged and must satisfy the callback's own lifetime contract.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_visit_owners_v1(
    scope: *const ElephcEvalScope, visitor: Option<OwnerVisitor>, context: *mut c_void,
) {
    let (Some(scope), Some(visitor)) = (unsafe { scope.as_ref() }, visitor) else { return; };
    scope.visit_owned_cell_slots(|slot| unsafe { visitor(slot, 0, context) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::ScopeCellOwnership;
    use crate::value::RuntimeCellHandle;

    /// Records only native pointer identities; these synthetic values are never dereferenced.
    unsafe extern "C" fn record(slot: *const *mut RuntimeCell, tag: u64, context: *mut c_void) {
        assert_eq!(tag, 0);
        let seen = unsafe { &mut *context.cast::<Vec<(usize, usize)>>() };
        seen.push((slot as usize, unsafe { *slot } as usize));
    }

    /// Equal pointers in independent owners remain distinct slots; borrowed and unset entries vanish.
    #[test]
    fn suspended_eval_scope_visitor_borrows_exact_owned_slots_without_mutating_them() {
        let mut scope = ElephcEvalScope::new();
        let same = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        scope.set("first", same, ScopeCellOwnership::Owned);
        scope.set("second", same, ScopeCellOwnership::Owned);
        scope.set("borrowed", same, ScopeCellOwnership::Borrowed);
        scope.set("gone", same, ScopeCellOwnership::Owned);
        assert_eq!(scope.unset("gone"), Some(same));
        let mut seen = Vec::<(usize, usize)>::new();
        let context = (&mut seen as *mut Vec<(usize, usize)>).cast();
        unsafe { __elephc_eval_scope_visit_owners_v1(&scope, Some(record), context); }
        assert_eq!(seen.len(), 2);
        assert_ne!(seen[0].0, seen[1].0, "each real owner has its own collector identity");
        assert!(seen.iter().all(|(_, value)| *value == 8));
        let before = seen.clone();
        seen.clear();
        unsafe { __elephc_eval_scope_visit_owners_v1(&scope, Some(record), context); }
        assert_eq!(seen, before, "walking cannot retain, clear or move an owning slot");
        assert_eq!(scope.drain_owned_cells().len(), 2);
    }
}
