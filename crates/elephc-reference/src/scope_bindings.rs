//! Purpose:
//! Keeps activation-owned local contracts at stable native reference binding addresses.
//!
//! Called from:
//! - Native eval scope publication and retirement through the versioned C ABI.
//!
//! Key details:
//! - Imported and explicitly open producers never acquire a later inferred contract.
//! - Registry operations perform no PHP, retain no cell themselves and finish before callbacks.
//! - Native callers own the cell in each returned slot and retire cells returned by removal.

use std::{cell::RefCell, collections::HashMap};

/// Stable physical binding layout consumed by native source authentication.
#[repr(C)]
#[derive(Default)]
struct Binding {
    cell: usize,
    validator: usize,
}

/// An explicit open origin is distinct from an as-yet unseen variable name.
#[derive(Default)]
struct ScopeBindings {
    scopes: HashMap<usize, HashMap<Vec<u8>, Option<Box<Binding>>>>,
}

impl ScopeBindings {
    /// Freezes the first producer contract, preserving an imported or explicitly open origin.
    fn binding(&mut self, scope: usize, name: &[u8], validator: usize) -> usize {
        let entry = self.scopes.entry(scope).or_default().entry(name.to_vec())
            .or_insert_with(|| (validator != 0).then(|| Box::new(Binding { cell: 0, validator })));
        entry.as_mut().map_or(0, |binding| &mut **binding as *mut Binding as usize)
    }

    /// Copies immutable origin metadata without sharing its binding or cell ownership.
    fn validator(&self, scope: usize, name: &[u8]) -> usize {
        self.scopes.get(&scope).and_then(|bindings| bindings.get(name))
            .and_then(Option::as_ref).map_or(0, |binding| binding.validator)
    }

    /// Detaches one physical source before its stable slot or native cell can be retired.
    fn take(&mut self, scope: usize, name: Option<&[u8]>) -> usize {
        let Some(bindings) = self.scopes.get_mut(&scope) else { return 0; };
        let key = name.map(<[u8]>::to_vec).or_else(|| bindings.keys().next().cloned());
        let removed = key.and_then(|key| bindings.remove(&key));
        if bindings.is_empty() { self.scopes.remove(&scope); }
        let Some(Some(binding)) = removed else { return 0; };
        let address = &*binding as *const Binding as usize;
        let status = super::native_reference_sources::__elephc_reference_sources_unbind_v1(address);
        assert_eq!(status, 0, "scope binding must detach before retirement");
        binding.cell
    }

    /// Borrows exact native cell owner slots while collector callbacks remain pure and nonmutating.
    fn visit(&self, scope: usize, mut visitor: impl FnMut(*const usize)) {
        if let Some(bindings) = self.scopes.get(&scope) {
            for binding in bindings.values().flatten().filter(|binding| binding.cell != 0) {
                visitor(&binding.cell);
            }
        }
    }

    /// Removes open origins while finding the next live native owner for a scope destructor.
    fn pop(&mut self, scope: usize) -> usize {
        while self.scopes.contains_key(&scope) {
            let cell = self.take(scope, None);
            if cell != 0 { return cell; }
        }
        0
    }
}

thread_local! {
    /// Stable slots live only on the worker that owns the corresponding native activation.
    static BINDINGS: RefCell<ScopeBindings> = RefCell::new(ScopeBindings::default());
}

/// Returns a stable two-word slot or zero for an imported or explicitly open binding.
///
/// # Safety
/// Scope identity must remain live until retirement. Name bytes are readable for the given length.
/// A nonzero validator points to immutable native declaration metadata. Native code initializes
/// and owns the slot's cell, and must not retain the slot address after taking that binding.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_scope_binding_v1(
    scope: usize, name: *const u8, len: usize, validator: usize,
) -> usize {
    if scope == 0 || (name.is_null() && len != 0) { return 0; }
    let name = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(name, len) } };
    BINDINGS.with(|bindings| bindings.borrow_mut().binding(scope, name, validator))
}

/// Copies the immutable validator descriptor for an independent value binding, or returns zero.
///
/// # Safety
/// Scope identity remains live and name bytes are readable for the supplied length.
/// A returned descriptor is immutable generated program metadata, independent of the scope slot.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_scope_type_v1(
    scope: usize, name: *const u8, len: usize,
) -> usize {
    if scope == 0 || (name.is_null() && len != 0) { return 0; }
    let name = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(name, len) } };
    BINDINGS.with(|bindings| bindings.borrow().validator(scope, name))
}

/// Detaches one source and transfers its native cell owner to the caller, or returns zero.
///
/// # Safety
/// Name bytes are readable for the given length. No native callback may still use the retired
/// slot. The caller consumes the returned cell exactly once through the managed-cell release ABI.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_scope_take_v1(
    scope: usize, name: *const u8, len: usize,
) -> usize {
    if name.is_null() && len != 0 { return 0; }
    let name = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(name, len) } };
    BINDINGS.with(|bindings| bindings.borrow_mut().take(scope, Some(name)))
}

/// Detaches the next scope owner, discarding open origins, or returns zero when fully retired.
#[no_mangle]
pub extern "C" fn __elephc_reference_scope_pop_v1(scope: usize) -> usize {
    BINDINGS.with(|bindings| bindings.borrow_mut().pop(scope))
}

/// Visits native scope cell owners without retaining values or lending mutable registry state.
///
/// # Safety
/// The scope and its slots remain live and immobile for this call. The callback performs only
/// collector bookkeeping: it cannot invoke PHP, unwind, mutate the registry or retain addresses.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_scope_visit_v1(scope: usize,
    visitor: Option<unsafe extern "C" fn(*const usize, u64, *mut std::ffi::c_void)>,
    context: *mut std::ffi::c_void,
) {
    let Some(visitor) = visitor else { return; };
    BINDINGS.with(|bindings| bindings.borrow().visit(scope, |slot| unsafe { visitor(slot, 0, context) }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Descriptor copies survive source retirement while open and absent origins stay open.
    #[test]
    fn scope_binding_descriptor_copies_have_independent_lifetimes() {
        let mut scopes = ScopeBindings::default();
        scopes.binding(5, b"source", 123);
        let copied = scopes.validator(5, b"source");
        scopes.binding(5, b"copy", copied);
        assert_eq!(scopes.take(5, Some(b"source")), 0);
        assert_eq!(scopes.validator(5, b"copy"), 123);
        scopes.binding(5, b"source", 456);
        scopes.binding(5, b"open", 0);
        assert_eq!(scopes.validator(5, b"source"), 456);
        assert_eq!(scopes.validator(5, b"copy"), 123);
        assert_eq!(scopes.validator(5, b"open"), 0);
        assert_eq!(scopes.validator(5, b"absent"), 0);
        assert_eq!(scopes.validator(6, b"copy"), 0);
    }

    /// Identical fragments keep activation origins separate and map growth preserves native slots.
    #[test]
    fn scope_binding_origins_keep_stable_slots_and_open_imports() {
        let mut scopes = ScopeBindings::default();
        let first = scopes.binding(1, b"value", 10);
        assert_ne!(first, 0);
        for i in 0..1024 { scopes.binding(1, i.to_string().as_bytes(), 20); }
        assert_eq!(scopes.binding(1, b"value", 30), first);
        assert_eq!(unsafe { (*(first as *const Binding)).validator }, 10);
        assert_eq!(scopes.binding(2, b"value", 0), 0);
        assert_eq!(scopes.binding(2, b"value", 10), 0);
        assert_eq!(scopes.pop(1), 0);
        assert_eq!(scopes.pop(2), 0);
        assert!(scopes.scopes.is_empty());
    }

    /// Independent owning slots remain independent even when their borrowed cell pointers agree.
    #[test]
    fn scope_binding_visitors_preserve_exact_owner_slot_identity() {
        let mut scopes = ScopeBindings::default();
        for name in [b"first".as_slice(), b"second"] {
            let slot = scopes.binding(4, name, 123);
            unsafe { (*(slot as *mut Binding)).cell = 456; }
        }
        scopes.binding(4, b"open", 0);
        scopes.binding(4, b"pending", 123);
        let mut seen = Vec::new();
        scopes.visit(4, |slot| seen.push((slot as usize, unsafe { *slot })));
        assert_eq!(seen.len(), 2);
        assert_ne!(seen[0].0, seen[1].0);
        assert!(seen.iter().all(|(_, cell)| *cell == 456));
        assert_eq!(scopes.pop(4), 456);
        assert_eq!(scopes.pop(4), 456);
        assert_eq!(scopes.pop(4), 0);
        assert!(scopes.scopes.is_empty());
    }

    /// Unset removes a declaration source before the old owner and permits a fresh origin.
    #[test]
    fn scope_binding_retirement_unbinds_before_owner_transfer() {
        let mut scopes = ScopeBindings::default();
        let slot = scopes.binding(3, b"value", 123);
        unsafe { (*(slot as *mut Binding)).cell = 456; }
        let token = super::super::native_reference_sources::__elephc_reference_sources_bind_v1(slot, 456, 123);
        assert_ne!(token, 0);
        assert_eq!(scopes.take(3, Some(b"value")), 456);
        assert_eq!(scopes.take(3, Some(b"value")), 0);
        let next = scopes.binding(3, b"value", 789);
        assert_eq!(unsafe { (*(next as *const Binding)).validator }, 789);
        assert_eq!(scopes.pop(3), 0);
        assert!(scopes.scopes.is_empty());
    }
}
