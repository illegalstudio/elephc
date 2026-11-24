//! Purpose:
//! Owns opaque native cell handles, runtime owner slots and protected retirement.
//!
//! Called from:
//! - Magician's scope, interpreter, and the eval FFI adapters.
//!
//! Key details:
//! - Handles carry Rust-only result provenance; native ABI slots contain raw pointers only.
//! - Copying a handle does not retain its cell. Borrowed reads must acquire a lease before cleanup.

use std::{cell::{Cell, RefCell}, ffi::c_void};

/// Opaque pointer to an elephc runtime cell.
pub type RuntimeCell = c_void;

/// Wraps a runtime pointer and records whether an expression borrows an existing owner's cell.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeCellHandle {
    ptr: *mut RuntimeCell,
    borrowed: bool,
}

impl RuntimeCellHandle {
    /// Accepts ownership transferred through a raw runtime-cell pointer.
    ///
    /// Storage lookup paths must explicitly mark their returned view as borrowed.
    pub const fn from_raw(ptr: *mut RuntimeCell) -> Self {
        Self { ptr, borrowed: false }
    }

    /// Returns the raw runtime-cell pointer for ABI calls back into elephc.
    pub const fn as_ptr(self) -> *mut RuntimeCell {
        self.ptr
    }

    /// Borrows the actual pointer field for a pure collector owner-slot callback.
    /// The enclosing owner must stay immobile and unchanged through the callback.
    pub fn owner_slot(&self) -> *const *mut RuntimeCell {
        &self.ptr
    }

    /// Returns true when this handle does not reference a runtime cell.
    pub const fn is_null(self) -> bool {
        self.ptr.is_null()
    }

    /// Marks a storage read as borrowed without changing the runtime reference count.
    pub const fn borrowed(self) -> Self {
        Self { borrowed: true, ..self }
    }

    /// Returns whether the expression must retain this cell before consuming it.
    pub const fn is_borrowed(self) -> bool {
        self.borrowed
    }
}

impl PartialEq for RuntimeCellHandle {
    /// Compares cell identity independently of how the current expression obtained its handle.
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl Eq for RuntimeCellHandle {}

/// One actual Rust owner whose address remains stable across the native call and cleanup.
pub struct RuntimeOwnerSlot(Cell<RuntimeCellHandle>);

// Every mutation is one complete `Cell::replace`, so unwinding can observe only the old or
// new owner record, never a partially published cleanup obligation.
impl std::panic::RefUnwindSafe for RuntimeOwnerSlot {}
impl std::panic::UnwindSafe for RuntimeOwnerSlot {}

/// Borrows actual owning fields while a frozen activation cannot mutate its inventory.
pub trait RuntimeOwners {
    /// Reports each independent lease without acquiring a reference or executing PHP.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell));
}

impl RuntimeOwners for RuntimeCellHandle {
    /// Borrowed metadata contributes no owner beyond its original storage slot.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        if !self.is_borrowed() && !self.is_null() { visit(self.owner_slot()); }
    }
}

impl<T: RuntimeOwners> RuntimeOwners for [T] {
    /// Walks the actual stable element fields rather than copying pointer values.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        for owner in self { owner.visit_owned_slots(visit); }
    }
}

impl<T: RuntimeOwners> RuntimeOwners for Vec<T> {
    /// Borrows vector storage, whose shape remains immutable through the guarded operation.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.as_slice().visit_owned_slots(visit);
    }
}

impl<T: RuntimeOwners + ?Sized> RuntimeOwners for &T {
    /// Forwards a borrowed inventory while preserving every original slot identity.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        (**self).visit_owned_slots(visit);
    }
}

impl<A: RuntimeOwners, B: RuntimeOwners> RuntimeOwners for (A, B) {
    /// Combines separate owner inventories without introducing duplicate leases.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.0.visit_owned_slots(visit);
        self.1.visit_owned_slots(visit);
    }
}

impl<T: RuntimeOwners> RuntimeOwners for Option<T> {
    /// An absent result or failed operation contributes no owning field.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        if let Some(owner) = self { owner.visit_owned_slots(visit); }
    }
}

impl RuntimeOwnerSlot {
    /// Returns the borrowed null sentinel used for a slot without an owner.
    fn vacant() -> RuntimeCellHandle {
        RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed()
    }

    /// Creates an owner slot that currently publishes no cleanup obligation.
    pub fn empty() -> Self {
        Self::new(Self::vacant())
    }

    /// Moves one cleanup obligation, preserving borrowed provenance when no lease exists.
    pub fn new(value: RuntimeCellHandle) -> Self { Self(Cell::new(value)) }

    /// Lends the cell to a native consumer without transferring its owning lease.
    pub fn borrowed(&self) -> RuntimeCellHandle { self.0.get().borrowed() }

    /// Detaches the owner before release can execute PHP or suspend again.
    pub fn take(&self) -> RuntimeCellHandle {
        self.0.replace(Self::vacant())
    }

    /// Detaches the owner while representing an already-empty slot as `None`.
    pub fn take_optional(&self) -> Option<RuntimeCellHandle> {
        let value = self.take();
        (!value.is_null()).then_some(value)
    }

    /// Publishes an optional replacement before returning the detached previous obligation.
    pub fn replace(&self, value: Option<RuntimeCellHandle>) -> Option<RuntimeCellHandle> {
        let value = value.unwrap_or_else(Self::vacant);
        let previous = self.0.replace(value);
        (!previous.is_null()).then_some(previous)
    }

    /// Updates an existing owner after a mutation returned its receiver without a new lease.
    pub fn update_receiver(&self, receiver: RuntimeCellHandle) {
        debug_assert!(!self.is_empty() && !self.0.get().is_borrowed());
        self.0.set(RuntimeCellHandle::from_raw(receiver.as_ptr()));
    }

    /// Transfers the result lease or discards it when construction or cleanup failed.
    pub fn finish<E>(
        &self,
        operation: Result<(), E>,
        release: impl FnOnce(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<RuntimeCellHandle, E> {
        let output = self.take();
        if operation.is_err() && !output.is_null() && !output.is_borrowed() {
            let _ = release(output);
        }
        operation.map(|()| output)
    }

    /// Reports whether this slot has already relinquished its lease.
    pub fn is_empty(&self) -> bool {
        self.0.get().is_null()
    }
}

impl Default for RuntimeOwnerSlot {
    /// Starts with no owner so resource records can fill their slots transactionally.
    fn default() -> Self { Self::empty() }
}

impl RuntimeOwners for RuntimeOwnerSlot {
    /// Resolves the actual pointer field during a pure collector visit.
    #[allow(unsafe_code)]
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        let cell = self.0.get();
        if !cell.is_borrowed() && !cell.is_null() {
            // A raw field address does not borrow the Cell's contents. The consumer may
            // only read it while this slot stays immobile and unchanged.
            visit(unsafe { &raw const (*self.0.as_ptr()).ptr });
        }
    }
}

/// Releases every argument in caller order while preserving an invocation's rooted result.
///
/// The caller keeps the result and argument slots visible to any collector invoked by
/// `release`. An invocation error wins over cleanup errors. Otherwise the first cleanup
/// error wins and the unpublished result is detached and discarded.
pub fn finish_argument_cleanup<'a, T: 'a, E: Copy, S>(
    result: &Result<RuntimeOwnerSlot, E>,
    owners: impl IntoIterator<Item = &'a T>,
    state: &mut S,
    mut release: impl FnMut(&mut S, &T) -> Result<(), E>,
    discard: impl FnOnce(&mut S, RuntimeCellHandle),
) -> Result<RuntimeCellHandle, E> {
    finish_owner_cleanup(
        result.as_ref().map_err(|status| *status),
        owners,
        CleanupErrorPolicy::First,
        state,
        &mut release,
        |state, result| discard(state, result.take()),
    )
    .map(RuntimeOwnerSlot::take)
}

/// Retires owners in iterator order while the caller keeps its result rooted.
/// The operation error takes precedence. A successful result is discarded on cleanup failure.
pub fn finish_owner_cleanup<R, T, E, S>(
    result: Result<R, E>,
    owners: impl IntoIterator<Item = T>,
    errors: CleanupErrorPolicy,
    state: &mut S,
    mut release: impl FnMut(&mut S, T) -> Result<(), E>,
    discard: impl FnOnce(&mut S, R),
) -> Result<R, E> {
    let cleanup = release_in_order(owners, errors, |owner| release(state, owner));
    match (result, cleanup) {
        (Err(status), _) => Err(status),
        (Ok(result), Err(status)) => {
            discard(state, result);
            Err(status)
        }
        (Ok(result), Ok(())) => Ok(result),
    }
}

/// Temporary leases accumulated as a construction performs fallible native calls.
/// A collector may inspect the live slots between stages and during retirement.
#[derive(Default)]
pub struct StagedRuntimeOwners(RefCell<Vec<RuntimeOwnerSlot>>);

impl StagedRuntimeOwners {
    /// Records an owned temporary for later cleanup and returns the cell for borrowing.
    pub fn stage(&self, cell: RuntimeCellHandle) -> RuntimeCellHandle {
        if !cell.is_borrowed() && !cell.is_null() {
            self.0.borrow_mut().push(RuntimeOwnerSlot::new(cell));
        }
        cell
    }

    /// Consumes every staged lease in acquisition order, retaining the first cleanup failure.
    pub fn release<E>(
        &self,
        operation: Result<(), E>,
        release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<(), E> {
        let count = self.0.borrow().len();
        let cells = (0..count).map(|index| self.0.borrow()[index].take());
        release_staged_cells(operation, cells, release)
    }

    /// Detaches each temporary immediately before release, with an explicit error policy.
    pub fn release_with_policy<E>(
        &self,
        errors: CleanupErrorPolicy,
        release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<(), E> {
        let count = self.0.borrow().len();
        let cells = (0..count).filter_map(|index| self.0.borrow()[index].take_optional());
        release_in_order(cells, errors, release)
    }

    /// Drains the temporaries, transferring the result only if construction and cleanup succeed.
    pub fn finish<E>(
        &self,
        operation: Result<(), E>,
        result: &RuntimeOwnerSlot,
        mut release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<RuntimeCellHandle, E> {
        let operation = self.release(operation, &mut release);
        result.finish(operation, release)
    }
}

impl RuntimeOwners for StagedRuntimeOwners {
    /// Makes only the outstanding leases visible to a pure collector visitor.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.0.borrow().visit_owned_slots(visit);
    }
}

/// Retires detached temporaries in the supplied order, retaining the original failure.
pub fn release_staged_cells<E>(
    mut operation: Result<(), E>,
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    mut release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
) -> Result<(), E> {
    for cell in cells {
        if !cell.is_null() && !cell.is_borrowed() {
            operation = operation.and(release(cell));
        }
    }
    operation
}

/// Consumes every lease through a contained native callback, accumulating its owned Throwable.
///
/// The callback returns whether this release failed. Previously accumulated exceptions can
/// survive successful releases without setting the failure bit.
pub fn release_owned_cells(
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    pending: Option<RuntimeCellHandle>,
    mut release: impl FnMut(*mut RuntimeCell, &mut *mut RuntimeCell) -> bool,
) -> (Option<RuntimeCellHandle>, bool) {
    let mut thrown = pending.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    let mut caught = false;
    for cell in cells {
        caught |= release(cell.as_ptr(), &mut thrown);
    }
    ((!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown)), caught)
}

/// Drains detached owner batches until release callbacks leave the source empty.
pub fn drain_owned_cell_batches<S, P>(
    source: &mut S,
    mut pending: P,
    mut take: impl FnMut(&mut S) -> Vec<RuntimeCellHandle>,
    mut release: impl FnMut(Vec<RuntimeCellHandle>, P) -> P,
) -> P {
    loop {
        let owners = take(source);
        if owners.is_empty() {
            return pending;
        }
        pending = release(owners, pending);
    }
}

/// Direction in which a detached batch retires its independent owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseOrder {
    /// Retires owners in the order supplied by the caller.
    Forward,
    /// Retires the most recently supplied owner first.
    Reverse,
}

/// Selects which cleanup error survives after every owner has been retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupErrorPolicy {
    /// Preserves the first cleanup error while continuing through the batch.
    First,
    /// Preserves the last cleanup error while continuing through the batch.
    Last,
    /// Discards cleanup errors after attempting every release.
    Ignore,
}

/// Releases a detached owner batch under an explicit ordering and error policy.
pub fn release_all<T, E>(
    owners: impl IntoIterator<Item = T>,
    order: ReleaseOrder,
    errors: CleanupErrorPolicy,
    release: impl FnMut(T) -> Result<(), E>,
) -> Result<(), E> {
    let mut owners: Vec<_> = owners.into_iter().collect();
    if order == ReleaseOrder::Reverse {
        owners.reverse();
    }
    release_in_order(owners, errors, release)
}

/// Retires owners in their supplied order, acquiring each immediately before release.
pub fn release_in_order<T, E>(
    owners: impl IntoIterator<Item = T>,
    errors: CleanupErrorPolicy,
    mut release: impl FnMut(T) -> Result<(), E>,
) -> Result<(), E> {
    let mut retained_error = None;
    for owner in owners {
        if let Err(error) = release(owner) {
            match errors {
                CleanupErrorPolicy::First if retained_error.is_none() => retained_error = Some(error),
                CleanupErrorPolicy::Last => retained_error = Some(error),
                CleanupErrorPolicy::Ignore | CleanupErrorPolicy::First => {}
            }
        }
    }
    retained_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps a result rooted through every release and preserves the first failure.
    #[test]
    fn argument_cleanup_keeps_the_result_live_and_preserves_error_order() {
        let value = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let result = Ok(RuntimeOwnerSlot::new(value));
        let slot = result.as_ref().unwrap();
        let mut released = Vec::new();

        let outcome = finish_argument_cleanup(
            &result,
            &[1, 2, 3],
            &mut released,
            |released, &owner| {
                assert!(!slot.is_empty());
                released.push(owner);
                Err(owner)
            },
            |released, discarded| {
                assert_eq!(discarded, value);
                assert!(slot.is_empty());
                released.push(4);
            },
        );
        assert_eq!(outcome, Err(1));
        assert_eq!(released, [1, 2, 3, 4]);

        let outcome = finish_argument_cleanup(
            &Err(5),
            &[1],
            &mut released,
            |released, &owner| { released.push(owner); Err(owner) },
            |_, _| panic!("a failed invocation has no result to discard"),
        );
        assert_eq!(outcome, Err(5));
        assert_eq!(released, [1, 2, 3, 4, 1]);

        released.clear();
        let slot = RuntimeOwnerSlot::new(value);
        let outcome = finish_owner_cleanup(
            Ok(&slot),
            [1, 2, 3].into_iter().rev(),
            CleanupErrorPolicy::Last,
            &mut released,
            |released, owner| {
                assert!(!slot.is_empty());
                released.push(owner);
                Err(owner)
            },
            |released, result| {
                assert_eq!(result.take(), value);
                released.push(4);
            },
        );
        assert_eq!(outcome.err(), Some(1));
        assert_eq!(released, [3, 2, 1, 4]);
    }

    /// Retires every staged lease and drops an unpublished result even when cleanup fails.
    #[test]
    fn staged_cleanup_preserves_operation_error_and_releases_partial_result() {
        let cell = |address| RuntimeCellHandle::from_raw(address as *mut RuntimeCell);
        let first = cell(8usize);
        let second = cell(16usize);
        let output = RuntimeOwnerSlot::new(cell(24usize));
        let owners = StagedRuntimeOwners::default();
        owners.stage(first);
        owners.stage(cell(32usize).borrowed());
        owners.stage(second);
        let mut released = Vec::new();

        let outcome = owners.finish(Err("construction"), &output, |retired| {
            let mut visible = 0;
            owners.visit_owned_slots(&mut |_| visible += 1);
            if retired == first {
                assert_eq!(visible, 1);
                assert!(!output.is_empty());
            }
            released.push(retired);
            Err("cleanup")
        });
        assert_eq!(outcome, Err("construction"));
        assert_eq!(released, [first, second, cell(24usize)]);
        assert!(output.is_empty());
    }

    /// The last release error wins while all later leases remain visible until retirement.
    #[test]
    fn staged_last_error_keeps_unretired_owners_visible() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let owners = StagedRuntimeOwners::default();
        owners.stage(first);
        owners.stage(second);
        let mut order = Vec::new();

        let cleanup = owners.release_with_policy(CleanupErrorPolicy::Last, |cell| {
            order.push(cell);
            let mut live = Vec::new();
            owners.visit_owned_slots(&mut |field| live.push(field));
            assert_eq!(live.len(), usize::from(cell == first));
            Err(cell)
        });
        assert_eq!(cleanup, Err(second));
        assert_eq!(order, [first, second]);
    }

    /// Reentrant release callbacks can publish a second batch without leaking it.
    #[test]
    fn detached_cell_batches_drain_to_quiescence() {
        use std::collections::VecDeque;
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let pending = RefCell::new(VecDeque::from([vec![first]]));
        let mut released = Vec::new();

        drain_owned_cell_batches(
            &mut &pending,
            (),
            |pending| pending.borrow_mut().pop_front().unwrap_or_default(),
            |batch, ()| {
                if batch == [first] {
                    pending.borrow_mut().push_back(vec![second]);
                }
                released.extend(batch);
            },
        );
        assert_eq!(released, [first, second]);
    }

    /// Releases every owner while honoring direction and retained-error selection.
    #[test]
    fn release_all_applies_explicit_order_and_error_policy() {
        let mut released = Vec::new();
        let outcome = release_all(
            [1, 2, 3], ReleaseOrder::Reverse, CleanupErrorPolicy::First,
            |owner| { released.push(owner); Err(owner) },
        );
        assert_eq!(released, [3, 2, 1]);
        assert_eq!(outcome, Err(3));

        released.clear();
        let outcome = release_all(
            [1, 2, 3], ReleaseOrder::Forward, CleanupErrorPolicy::Last,
            |owner| { released.push(owner); Err(owner) },
        );
        assert_eq!(released, [1, 2, 3]);
        assert_eq!(outcome, Err(3));

        released.clear();
        let outcome = release_all(
            [1, 2, 3],
            ReleaseOrder::Forward,
            CleanupErrorPolicy::Ignore,
            |owner| {
                released.push(owner);
                Err(owner)
            },
        );
        assert_eq!(released, [1, 2, 3]);
        assert_eq!(outcome, Ok(()));
    }

    /// Publishes a replacement owner before handing the preceding lease to cleanup.
    #[test]
    fn owner_slot_replacement_detaches_the_previous_owner() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let slot = RuntimeOwnerSlot::new(first);
        assert_eq!(slot.replace(Some(second)), Some(first));
        assert_eq!(slot.borrowed(), second.borrowed());
        assert_eq!(slot.take_optional(), Some(second));
        assert_eq!(slot.take_optional(), None);
        assert!(slot.is_empty());
    }
}
