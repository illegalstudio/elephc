//! Purpose:
//! Owns opaque native cell handles, runtime owner slots and protected retirement.
//!
//! Called from:
//! - Magician's scope, interpreter, and the eval FFI adapters.
//!
//! Key details:
//! - Handles carry Rust-only result provenance; native ABI slots contain raw pointers only.
//! - Copying a handle does not retain its cell. Borrowed reads must acquire a lease before cleanup.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    hash::Hash,
};

/// Opaque pointer to an elephc runtime cell.
pub type RuntimeCell = c_void;

/// Detached cell owners that must be retired as one explicit cleanup batch.
#[derive(Default)]
#[must_use = "detached runtime owners must be transferred or released"]
pub struct RuntimeOwnerBatch {
    cells: Vec<RuntimeOwnerSlot>,
}

impl RuntimeOwnerBatch {
    /// Creates an empty detached batch.
    pub const fn empty() -> Self {
        Self { cells: Vec::new() }
    }

    /// Collects detached owners in their semantic acquisition order.
    pub fn new(cells: impl IntoIterator<Item = RuntimeCellHandle>) -> Self {
        Self {
            cells: cells.into_iter().map(RuntimeOwnerSlot::new).collect(),
        }
    }

    /// Reports whether the batch carries no cleanup obligations.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Returns the number of detached cleanup obligations.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Borrows the detached owners without transferring the batch obligation.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = RuntimeCellHandle> + '_ {
        self.cells
            .iter()
            .map(RuntimeOwnerSlot::borrowed)
            .filter(|cell| !cell.is_null())
    }

    /// Appends another detached inventory without releasing between batches.
    pub fn extend(&mut self, cells: impl IntoIterator<Item = RuntimeCellHandle>) {
        self.cells
            .extend(cells.into_iter().map(RuntimeOwnerSlot::new));
    }

    /// Appends one detached owner at the end of the semantic acquisition order.
    pub fn push(&mut self, cell: RuntimeCellHandle) {
        self.cells.push(RuntimeOwnerSlot::new(cell));
    }

    /// Detaches the newest outstanding owner while preserving every older batch slot.
    pub fn pop(&mut self) -> Option<RuntimeCellHandle> {
        while let Some(slot) = self.cells.pop() {
            if let Some(cell) = slot.take_optional() {
                return Some(cell);
            }
        }
        None
    }

    /// Appends another detached batch without exposing either inventory as raw handles.
    pub fn append(&mut self, mut other: Self) {
        self.cells.append(&mut other.cells);
    }

    /// Transfers every outstanding owner to one lower-level batch adapter.
    pub fn transfer(self) -> Vec<RuntimeCellHandle> {
        self.cells
            .iter()
            .filter_map(RuntimeOwnerSlot::take_optional)
            .collect()
    }

    /// Retires every owner under an explicit ordering and error policy.
    pub fn release<E>(
        &self,
        order: ReleaseOrder,
        errors: CleanupErrorPolicy,
        mut release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut indices = (0..self.cells.len()).collect::<Vec<_>>();
        if order == ReleaseOrder::Reverse {
            indices.reverse();
        }
        release_in_order(indices, errors, |index| {
            self.cells[index]
                .take_optional()
                .map_or(Ok(()), &mut release)
        })
    }

    /// Retires the batch before transferring a rooted result, discarding it on cleanup failure.
    pub fn finish<E>(
        &self,
        operation: Result<(), E>,
        result: &RuntimeOwnerSlot,
        order: ReleaseOrder,
        errors: CleanupErrorPolicy,
        mut release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<RuntimeCellHandle, E> {
        let cleanup = self.release(order, errors, &mut release);
        result.finish(operation.and(cleanup), release)
    }
}

impl From<Vec<RuntimeCellHandle>> for RuntimeOwnerBatch {
    /// Wraps an existing detached inventory without copying its owner handles.
    fn from(cells: Vec<RuntimeCellHandle>) -> Self {
        Self::new(cells)
    }
}

impl FromIterator<RuntimeCellHandle> for RuntimeOwnerBatch {
    /// Collects an iterator directly into an explicit detached-owner batch.
    fn from_iter<T: IntoIterator<Item = RuntimeCellHandle>>(iter: T) -> Self {
        Self::new(iter)
    }
}

impl IntoIterator for RuntimeOwnerBatch {
    type Item = RuntimeCellHandle;
    type IntoIter = std::vec::IntoIter<RuntimeCellHandle>;

    /// Transfers the detached obligations to a consumer that owns their cleanup protocol.
    fn into_iter(self) -> Self::IntoIter {
        self.transfer().into_iter()
    }
}

impl RuntimeOwners for RuntimeOwnerBatch {
    /// Visits outstanding slot fields so callback-driven collection sees every unretired owner.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.cells.visit_owned_slots(visit);
    }
}

/// Whether a persistent record must retire its runtime cell or merely observes another owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeCellOwnership {
    /// Another stable record owns the runtime reference.
    Borrowed,
    /// This record carries one cleanup obligation.
    Owned,
}

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

    /// Normalizes one handle as an owning lease or a borrowed observation.
    ///
    /// Callers selecting `Owned` must already hold the cleanup obligation represented by the
    /// handle. This changes only publication metadata and never acquires a runtime reference.
    pub const fn for_ownership(self, ownership: RuntimeCellOwnership) -> Self {
        match ownership {
            RuntimeCellOwnership::Borrowed => self.borrowed(),
            RuntimeCellOwnership::Owned => Self {
                borrowed: false,
                ..self
            },
        }
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

/// Send-safe storage for one owned runtime cell behind a process-global mutex.
///
/// The pointer is kept as an address so the container remains `Send` without claiming that
/// arbitrary runtime cells may be dereferenced from another thread. Consumers may borrow the
/// address for lookup or transfer the original cleanup obligation after removing the record.
#[derive(Debug, PartialEq, Eq)]
pub struct RuntimeAddressOwner {
    address: usize,
}

impl RuntimeAddressOwner {
    /// Erases one owned handle for storage in a Send-safe process registry.
    pub fn new(owner: RuntimeCellHandle) -> Self {
        debug_assert!(!owner.is_borrowed(), "a process registry requires an owned runtime cell");
        Self { address: owner.as_ptr() as usize }
    }

    /// Returns whether this record owns the same runtime-cell address.
    pub fn matches(&self, cell: RuntimeCellHandle) -> bool {
        self.address == cell.as_ptr() as usize
    }

    /// Lends the stored cell without transferring its cleanup obligation.
    pub fn borrowed(&self) -> RuntimeCellHandle {
        RuntimeCellHandle::from_raw(self.address as *mut RuntimeCell).borrowed()
    }

    /// Restores the owned handle after the enclosing registry detached this record.
    pub fn into_owned(self) -> RuntimeCellHandle {
        RuntimeCellHandle::from_raw(self.address as *mut RuntimeCell)
    }
}

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

    /// Classifies one persistent record as an owner or observer without exposing handle flags.
    pub fn for_ownership(value: RuntimeCellHandle, ownership: RuntimeCellOwnership) -> Self {
        Self::new(value.for_ownership(ownership))
    }

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
        self.0.set(receiver.for_ownership(RuntimeCellOwnership::Owned));
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

/// Persistent keyed owners with centralized distinct replacement and teardown transfer.
#[derive(Default)]
pub struct RuntimeOwnerMap<K> {
    entries: HashMap<K, RuntimeCellHandle>,
}

impl<K: Eq + Hash> RuntimeOwnerMap<K> {
    /// Returns whether the persistent inventory owns no cells.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns whether an owner is stored under the exact key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// Borrows the stored owner record without acquiring another runtime reference.
    pub fn get(&self, key: &K) -> Option<&RuntimeCellHandle> {
        self.entries.get(key)
    }

    /// Iterates stable key and owner references while the map remains immutable.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &RuntimeCellHandle)> {
        self.entries.iter()
    }

    /// Inserts a new owner when the caller has already proven the key is vacant.
    pub fn insert(&mut self, key: K, value: RuntimeCellHandle) {
        assert!(
            !self.entries.contains_key(&key),
            "RuntimeOwnerMap::insert requires a vacant key"
        );
        self.entries.insert(key, value);
    }

    /// Publishes a replacement and returns only a displaced distinct cleanup obligation.
    pub fn replace_distinct(
        &mut self,
        key: K,
        value: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.entries
            .insert(key, value)
            .filter(|previous| *previous != value)
    }

    /// Detaches every persistent owner before release callbacks can reenter its consumer.
    pub fn take_all(&mut self) -> RuntimeOwnerBatch {
        RuntimeOwnerBatch::new(self.entries.drain().map(|(_, cell)| cell))
    }
}

impl<K> RuntimeOwners for RuntimeOwnerMap<K> {
    /// Visits the actual map value slots while the inventory stays immutable.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        for cell in self.entries.values() {
            cell.visit_owned_slots(visit);
        }
    }
}

/// Persistent keyed owners paired with consumer-defined lifetime metadata.
pub struct RuntimeOwnerRecordMap<K, M> {
    entries: HashMap<K, (RuntimeCellHandle, M)>,
}

impl<K: Eq + Hash, M> RuntimeOwnerRecordMap<K, M> {
    /// Returns whether no owner records remain published.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns whether an owner record exists under the exact key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// Borrows the consumer metadata while the map retains its paired cell owner.
    pub fn metadata(&self, key: &K) -> Option<&M> {
        self.entries.get(key).map(|(_, metadata)| metadata)
    }

    /// Publishes one owner and its inseparable consumer metadata under a vacant key.
    pub fn insert(&mut self, key: K, owner: RuntimeCellHandle, metadata: M) {
        assert!(
            !self.entries.contains_key(&key),
            "RuntimeOwnerRecordMap::insert requires a vacant key"
        );
        self.entries.insert(key, (owner, metadata));
    }

    /// Detaches every owner and metadata record before callback-sensitive teardown.
    pub fn take_all(&mut self) -> Vec<(RuntimeCellHandle, M)> {
        self.entries.drain().map(|(_, record)| record).collect()
    }
}

impl<K, M> Default for RuntimeOwnerRecordMap<K, M> {
    /// Starts with no published owner records.
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<K, M> RuntimeOwners for RuntimeOwnerRecordMap<K, M> {
    /// Visits only the owned cell half of each record, never opaque metadata.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        for (cell, _) in self.entries.values() {
            cell.visit_owned_slots(visit);
        }
    }
}

/// Activation-owned cells whose clones are read-only snapshots, never duplicate owners.
#[derive(Debug)]
pub struct RuntimeOwnerList {
    cells: Vec<RuntimeCellHandle>,
    owns_cells: bool,
}

impl RuntimeOwnerList {
    /// Creates the one activation record responsible for retiring these cells.
    pub fn new(cells: Vec<RuntimeCellHandle>) -> Self {
        Self {
            cells,
            owns_cells: true,
        }
    }

    /// Borrows one cell without transferring the activation's cleanup obligation.
    pub fn get(&self, index: usize) -> Option<RuntimeCellHandle> {
        self.cells.get(index).copied().map(RuntimeCellHandle::borrowed)
    }

    /// Transfers owned cells to activation cleanup and leaves snapshots empty-handed.
    pub fn into_owned(mut self) -> RuntimeOwnerBatch {
        if self.owns_cells {
            self.owns_cells = false;
            RuntimeOwnerBatch::from(std::mem::take(&mut self.cells))
        } else {
            RuntimeOwnerBatch::empty()
        }
    }
}

impl Clone for RuntimeOwnerList {
    /// Copies lookup metadata while keeping the cleanup obligation on the original record.
    fn clone(&self) -> Self {
        Self {
            cells: self.cells.clone(),
            owns_cells: false,
        }
    }
}

impl PartialEq for RuntimeOwnerList {
    /// Compares PHP-visible argument cells independently of snapshot ownership provenance.
    fn eq(&self, other: &Self) -> bool {
        self.cells == other.cells
    }
}

impl Eq for RuntimeOwnerList {}

impl RuntimeOwners for RuntimeOwnerList {
    /// Snapshot lists expose no owners; the original activation exposes every live cell slot.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        if self.owns_cells {
            self.cells.visit_owned_slots(visit);
        }
    }
}

/// Active persistent owner metadata plus the exact restoration history beneath it.
pub struct RuntimeOwnerHistory<T> {
    active: Option<T>,
    stack: Vec<Option<T>>,
}

impl<T: Copy> RuntimeOwnerHistory<T> {
    /// Reports whether neither the active record nor any saved predecessor owns a value.
    pub fn is_empty(&self) -> bool {
        self.active.is_none() && self.stack.iter().all(Option::is_none)
    }

    /// Installs a replacement and saves the prior active record for restoration.
    pub fn push(&mut self, replacement: Option<T>) -> Option<T> {
        let previous = self.active;
        self.stack.push(previous);
        self.active = replacement;
        previous
    }

    /// Restores the preceding record and returns the discarded active record.
    pub fn restore(&mut self) -> Option<T> {
        let discarded = self.active.take();
        self.active = self.stack.pop().flatten();
        discarded
    }

    /// Returns the active record without transferring its cleanup obligation.
    pub const fn current(&self) -> Option<T> {
        self.active
    }

    /// Temporarily detaches the active record without altering restoration history.
    pub fn suspend(&mut self) -> Option<T> {
        self.active.take()
    }

    /// Restores a suspended record unless a nested callback already installed another.
    pub fn resume(&mut self, suspended: T) -> Option<T> {
        if self.active.is_none() {
            self.active = Some(suspended);
            None
        } else {
            Some(suspended)
        }
    }

    /// Detaches the active record and every saved predecessor before cleanup can reenter.
    pub fn take_all(&mut self) -> Vec<T> {
        self.active
            .take()
            .into_iter()
            .chain(self.stack.drain(..).flatten())
            .collect()
    }
}

impl<T> Default for RuntimeOwnerHistory<T> {
    /// Starts with neither an active owner record nor saved predecessors.
    fn default() -> Self {
        Self {
            active: None,
            stack: Vec::new(),
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
    /// Returns the number of stable capture slots, including slots already detached for cleanup.
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    /// Reports whether every allocated capture slot is currently vacant.
    pub fn is_empty(&self) -> bool {
        self.0.borrow().iter().all(RuntimeOwnerSlot::is_empty)
    }

    /// Records an owned temporary for later cleanup and returns the cell for borrowing.
    pub fn stage(&self, cell: RuntimeCellHandle) -> RuntimeCellHandle {
        if !cell.is_borrowed() && !cell.is_null() {
            self.0.borrow_mut().push(RuntimeOwnerSlot::new(cell));
        }
        cell
    }

    /// Borrows one staged cell without transferring its cleanup obligation.
    pub fn get(&self, index: usize) -> Option<RuntimeCellHandle> {
        self.0.borrow().get(index).and_then(|slot| {
            let cell = slot.borrowed();
            (!cell.is_null()).then_some(cell)
        })
    }

    /// Copies every live capture as a borrowed value list for a native consumer.
    pub fn borrowed_values(&self) -> Vec<RuntimeCellHandle> {
        self.0
            .borrow()
            .iter()
            .filter_map(|slot| {
                let cell = slot.borrowed();
                (!cell.is_null()).then_some(cell)
            })
            .collect()
    }

    /// Finds the last live capture with the same runtime-cell identity.
    pub fn rposition(&self, cell: RuntimeCellHandle) -> Option<usize> {
        self.0.borrow().iter().rposition(|slot| {
            let candidate = slot.borrowed();
            !candidate.is_null() && candidate == cell
        })
    }

    /// Detaches the first live capture with the same runtime-cell identity.
    pub fn take_first(&self, cell: RuntimeCellHandle) -> Option<RuntimeCellHandle> {
        let index = self.0.borrow().iter().position(|slot| {
            let candidate = slot.borrowed();
            !candidate.is_null() && candidate == cell
        })?;
        self.take_at(index)
    }

    /// Detaches one indexed capture while preserving every other capture index.
    pub fn take_at(&self, index: usize) -> Option<RuntimeCellHandle> {
        self.0.borrow().get(index).and_then(RuntimeOwnerSlot::take_optional)
    }

    /// Detaches the last allocated capture while preserving its now-vacant slot.
    pub fn take_last(&self) -> Option<RuntimeCellHandle> {
        self.0.borrow().last().and_then(RuntimeOwnerSlot::take_optional)
    }

    /// Publishes a replacement in one indexed slot before returning its previous owner.
    pub fn replace_at(
        &self,
        index: usize,
        replacement: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.0
            .borrow()
            .get(index)
            .and_then(|slot| slot.replace(Some(replacement)))
    }

    /// Transfers every outstanding lease in acquisition order without releasing it.
    pub fn take_all(&self) -> RuntimeOwnerBatch {
        let count = self.0.borrow().len();
        RuntimeOwnerBatch::new(
            (0..count).filter_map(|index| self.0.borrow()[index].take_optional()),
        )
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

    /// Runs cleanup before returning a caller-rooted or non-cell result.
    pub fn complete<R, E>(
        &self,
        operation: Result<R, E>,
        release: impl FnMut(RuntimeCellHandle) -> Result<(), E>,
    ) -> Result<R, E> {
        let cleanup = self.release(Ok(()), release);
        operation.and_then(|result| cleanup.map(|()| result))
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

/// Drains detached owner batches until release callbacks leave the source empty.
pub fn drain_owned_cell_batches<S, P>(
    source: &mut S,
    mut pending: P,
    mut take: impl FnMut(&mut S) -> RuntimeOwnerBatch,
    mut release: impl FnMut(RuntimeOwnerBatch, P) -> P,
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

    /// Process-global storage lends borrowed views and transfers its original cleanup obligation.
    #[test]
    fn runtime_address_owner_separates_borrow_from_transfer() {
        let cell = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let owner = RuntimeAddressOwner::new(cell);
        let borrowed = owner.borrowed();
        assert_eq!(borrowed, cell);
        assert!(borrowed.is_borrowed());
        let transferred = owner.into_owned();
        assert_eq!(transferred, cell);
        assert!(!transferred.is_borrowed());
    }

    /// Persistent slot classification keeps the cleanup obligation inside Memoir.
    #[test]
    fn runtime_owner_slot_classifies_persistent_ownership() {
        let cell = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell).borrowed();
        assert!(!cell.for_ownership(RuntimeCellOwnership::Owned).is_borrowed());
        assert!(cell.for_ownership(RuntimeCellOwnership::Borrowed).is_borrowed());
        let owner = RuntimeOwnerSlot::for_ownership(cell, RuntimeCellOwnership::Owned);
        let observer = RuntimeOwnerSlot::for_ownership(cell, RuntimeCellOwnership::Borrowed);
        assert!(!owner.take().is_borrowed());
        assert!(observer.take().is_borrowed());
    }

    /// Persistent maps detach only distinct displaced owners and drain exactly once.
    #[test]
    fn memoir_transfer_owner_map_centralizes_replacement_and_teardown() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let mut owners = RuntimeOwnerMap::default();
        owners.insert("key", first);
        assert_eq!(owners.replace_distinct("key", first), None);
        assert_eq!(owners.replace_distinct("key", second), Some(first));
        assert_eq!(owners.get(&"key"), Some(&second));
        assert_eq!(owners.take_all().iter().collect::<Vec<_>>(), vec![second]);
        assert!(owners.take_all().is_empty());
    }

    /// Batch composition keeps every outstanding slot rooted and honors reverse retirement.
    #[test]
    fn runtime_owner_batch_append_keeps_unretired_slots_visible() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let third = RuntimeCellHandle::from_raw(24usize as *mut RuntimeCell);
        let mut owners = RuntimeOwnerBatch::new([first]);
        owners.push(second);
        owners.append(RuntimeOwnerBatch::new([third]));
        assert_eq!(owners.iter().collect::<Vec<_>>(), [first, second, third]);

        let mut released = Vec::new();
        let result: Result<(), std::convert::Infallible> = owners.release(
            ReleaseOrder::Reverse,
            CleanupErrorPolicy::Ignore,
            |cell| {
                released.push(cell);
                let mut visible = 0;
                owners.visit_owned_slots(&mut |_| visible += 1);
                assert_eq!(visible, 3 - released.len());
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(released, [third, second, first]);
    }

    /// Batch completion retains the last cleanup error and discards its rooted result.
    #[test]
    fn runtime_owner_batch_finish_discards_result_after_cleanup_failure() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let output = RuntimeCellHandle::from_raw(24usize as *mut RuntimeCell);
        let owners = RuntimeOwnerBatch::new([first, second]);
        let result = RuntimeOwnerSlot::new(output);
        let mut released = Vec::new();

        let outcome = owners.finish(
            Ok(()),
            &result,
            ReleaseOrder::Forward,
            CleanupErrorPolicy::Last,
            |cell| {
                released.push(cell);
                Err(cell)
            },
        );
        assert_eq!(outcome, Err(second));
        assert_eq!(released, [first, second, output]);
        assert!(result.is_empty());
    }

    /// Record maps keep metadata paired with its cell through teardown transfer.
    #[test]
    fn memoir_transfer_owner_record_map_keeps_metadata_paired() {
        let cell = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let mut owners = RuntimeOwnerRecordMap::default();
        owners.insert("callback", cell, 41);
        assert!(owners.contains_key(&"callback"));
        assert_eq!(owners.metadata(&"callback"), Some(&41));
        assert_eq!(owners.take_all(), vec![(cell, 41)]);
        assert!(owners.is_empty());
    }

    /// Activation snapshots borrow cells while only the original transfers teardown owners.
    #[test]
    fn memoir_transfer_owner_list_clones_do_not_duplicate_cleanup() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let owners = RuntimeOwnerList::new(vec![first, second]);
        let snapshot = owners.clone();
        assert_eq!(snapshot.get(0), Some(first.borrowed()));
        assert!(snapshot.into_owned().is_empty());
        assert_eq!(
            owners.into_owned().iter().collect::<Vec<_>>(),
            vec![first, second],
        );
    }

    /// Handler history restores predecessors and transfers all remaining owners once.
    #[test]
    fn memoir_transfer_owner_history_preserves_restore_and_suspend() {
        let mut history = RuntimeOwnerHistory::default();
        assert!(history.is_empty());
        assert_eq!(history.push(Some(1)), None);
        assert!(!history.is_empty());
        assert_eq!(history.push(Some(2)), Some(1));
        assert_eq!(history.restore(), Some(2));
        assert_eq!(history.current(), Some(1));
        assert_eq!(history.suspend(), Some(1));
        assert_eq!(history.resume(1), None);
        assert_eq!(history.take_all(), vec![1]);
        assert!(history.take_all().is_empty());
        assert!(history.is_empty());
    }

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

    /// Indexed captures leave vacant slots so plans keep stable positions through retirement.
    #[test]
    fn staged_indexed_captures_preserve_positions() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let replacement = RuntimeCellHandle::from_raw(24usize as *mut RuntimeCell);
        let owners = StagedRuntimeOwners::default();
        assert!(owners.is_empty());
        owners.stage(first);
        owners.stage(second);
        assert_eq!(owners.len(), 2);
        assert_eq!(owners.rposition(first), Some(0));
        assert_eq!(owners.get(1), Some(second.borrowed()));
        assert_eq!(owners.replace_at(0, replacement), Some(first));
        assert_eq!(owners.take_first(replacement), Some(replacement));
        assert_eq!(owners.take_last(), Some(second));
        assert_eq!(owners.len(), 2);
        assert_eq!(owners.get(0), None);
        assert_eq!(owners.get(1), None);
        assert!(owners.is_empty());
        assert!(owners.take_all().is_empty());
    }

    /// Reentrant release callbacks can publish a second batch without leaking it.
    #[test]
    fn detached_cell_batches_drain_to_quiescence() {
        use std::collections::VecDeque;
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let pending = RefCell::new(VecDeque::from([RuntimeOwnerBatch::new([first])]));
        let mut released = Vec::new();

        drain_owned_cell_batches(
            &mut &pending,
            (),
            |pending| pending.borrow_mut().pop_front().unwrap_or_default(),
            |batch, ()| {
                if batch.iter().eq([first]) {
                    pending.borrow_mut().push_back(RuntimeOwnerBatch::new([second]));
                }
                let result: Result<(), std::convert::Infallible> = batch.release(
                    ReleaseOrder::Forward,
                    CleanupErrorPolicy::Ignore,
                    |cell| {
                        released.push(cell);
                        Ok(())
                    },
                );
                assert_eq!(result, Ok(()));
            },
        );
        assert_eq!(released, [first, second]);
    }

    /// A mutable work stack keeps older owners published while transferring its newest item.
    #[test]
    fn runtime_owner_batch_pops_in_reverse_order() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let mut owners = RuntimeOwnerBatch::new([first, second]);
        assert_eq!(owners.pop(), Some(second));
        assert_eq!(owners.iter().collect::<Vec<_>>(), vec![first]);
        assert_eq!(owners.pop(), Some(first));
        assert_eq!(owners.pop(), None);
        assert!(owners.is_empty());
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
