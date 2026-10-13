//! Purpose:
//! Owns native, PHP-pointer-free transfer envelopes for isolated Parallel workers.
//!
//! Called from:
//! - The Parallel runtime bridge before and after worker-context execution.
//!
//! Key details:
//! - Payload bytes are copied into a versioned native allocation and never borrow a PHP arena.
//! - The allocation crosses threads by ownership transfer and must be released exactly once.
//! - All exported functions catch Rust panics before returning across the C ABI.

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use elephc_parallel_contract::{
    buffer_registry_bytes, object_handle_free_slots, object_handle_index_slots,
    SERIALIZE_OBJECT_TABLE_SLOTS, UNSERIALIZE_VALUE_TABLE_SLOTS, WORKER_STACK_MAX_BYTES,
    WORKER_STACK_MIN_BYTES,
};
use std::ffi::c_void;

mod executor;
#[cfg(unix)]
mod completion_wake;
#[cfg(unix)]
mod periodic;
mod failure;
mod job;
mod php_wire;
mod value;

pub use failure::*;
pub use php_wire::*;
pub use value::*;

const TRANSFER_MAGIC: [u8; 8] = *b"EPAR\x01\0\0\0";
pub(crate) const TRANSFER_HEADER_LEN: usize = TRANSFER_MAGIC.len();

pub const PARALLEL_TRANSFER_OK: i32 = 0;
pub const PARALLEL_TRANSFER_INVALID_ARGUMENT: i32 = 1;
pub const PARALLEL_TRANSFER_ALLOCATION_FAILED: i32 = 2;
pub const PARALLEL_TRANSFER_INVALID_ENVELOPE: i32 = 3;
pub const PARALLEL_TRANSFER_PANIC: i32 = 4;
pub const PARALLEL_WORKER_INVALID_ARGUMENT: i64 = i64::MIN + 1;
pub const PARALLEL_WORKER_ALLOCATION_FAILED: i64 = i64::MIN + 2;
pub const PARALLEL_WORKER_CONTEXT_UNAVAILABLE: i64 = i64::MIN + 3;
pub const PARALLEL_WORKER_PANIC: i64 = i64::MIN + 4;
pub use elephc_parallel_contract::{
    PARALLEL_WORKER_CLEANUP_FAILED, PARALLEL_WORKER_PHP_FATAL,
};
#[cfg(test)]
pub(crate) const TEST_PANIC_CALLBACK_ADDR: usize = usize::MAX;

type ContextAcquire = unsafe extern "C" fn(
    *mut u8,
    usize,
    *mut u32,
    *mut u32,
    *mut u8,
    *mut u64,
    *mut u64,
    *mut u64,
) -> *mut c_void;
type ContextRelease = unsafe extern "C" fn(*mut c_void) -> i64;
type WorkerCallback = unsafe extern "C" fn(*mut c_void) -> i64;
const WORKER_ENTRY_RELEASE_NOT_ATTEMPTED: i64 = i64::MIN;
type WorkerEntry = unsafe extern "C" fn(
    *mut c_void,
    WorkerCallback,
    *mut c_void,
    usize,
    *mut i64,
) -> i64;

thread_local! {
    /// Depth is process-thread local: only the Rust thread currently executing isolated PHP may
    /// report itself as a Parallel worker. Parent threads and unrelated bridge callers stay zero.
    static PARALLEL_WORKER_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// A parent thread may own one live structured Parallel scope. This flag lets compiler-owned
    /// Fiber suspension guards reject user yields that would strand the scope outside its caller.
    static PARALLEL_PARENT_SCOPE_DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct ParallelWorkerGuard;

impl ParallelWorkerGuard {
    /// Marks the calling OS thread as executing isolated PHP until this guard is dropped.
    fn enter() -> Self {
        PARALLEL_WORKER_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for ParallelWorkerGuard {
    /// Restores the calling thread's worker depth on normal return or Rust unwinding.
    fn drop(&mut self) {
        PARALLEL_WORKER_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Returns one only while the calling OS thread is executing an isolated Parallel worker.
#[no_mangle]
pub extern "C" fn elephc_parallel_worker_active() -> i32 {
    PARALLEL_WORKER_DEPTH.with(|depth| i32::from(depth.get() != 0))
}

/// Enters the caller thread's structured parent scope, returning zero for forbidden nesting.
#[no_mangle]
pub extern "C" fn elephc_parallel_parent_scope_enter() -> i32 {
    PARALLEL_PARENT_SCOPE_DEPTH.with(|depth| {
        if depth.get() != 0 {
            return 0;
        }
        depth.set(1);
        1
    })
}

/// Leaves the caller thread's structured parent scope after normal or exceptional drain.
#[no_mangle]
pub extern "C" fn elephc_parallel_parent_scope_leave() -> i32 {
    PARALLEL_PARENT_SCOPE_DEPTH.with(|depth| {
        let was_active = depth.get() != 0;
        depth.set(0);
        i32::from(was_active)
    })
}

/// Returns one while a Parallel parent scope is active on the calling OS thread.
#[no_mangle]
pub extern "C" fn elephc_parallel_parent_scope_active() -> i32 {
    PARALLEL_PARENT_SCOPE_DEPTH.with(|depth| i32::from(depth.get() != 0))
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParallelTransferBlob {
    pub ptr: *mut u8,
    pub len: usize,
}

impl ParallelTransferBlob {
    pub(crate) const EMPTY: Self = Self {
        ptr: ptr::null_mut(),
        len: 0,
    };
}

/// Checks the native envelope header in storage whose readable range is guaranteed by the caller.
pub(crate) fn transfer_blob_is_valid(blob: ParallelTransferBlob) -> bool {
    if blob.ptr.is_null() || blob.len < TRANSFER_MAGIC.len() {
        return false;
    }
    let bytes = unsafe { std::slice::from_raw_parts(blob.ptr, blob.len) };
    bytes[..TRANSFER_MAGIC.len()] == TRANSFER_MAGIC
}

/// Consumes an owned native transfer allocation; a null descriptor is a no-op.
///
/// # Safety
/// A nonnull blob must describe an unreleased bridge allocation with its original length.
/// No borrowed view or other owner may access that allocation after release.
pub(crate) unsafe fn release_transfer_blob(blob: ParallelTransferBlob) {
    if blob.ptr.is_null() {
        return;
    }
    let slice = ptr::slice_from_raw_parts_mut(blob.ptr, blob.len);
    drop(unsafe { Box::from_raw(slice) });
}

/// Native memory owned by one isolated worker for the lifetime of its runtime context.
pub struct WorkerStorage {
    heap_size: usize,
    arena: Box<[u128]>,
    object_index: Box<[u32]>,
    object_free: Box<[u32]>,
    buffer_registry: Box<[u64]>,
    serialize_object_ptrs: Box<[u64]>,
    serialize_object_indexes: Box<[u64]>,
    unserialize_values: Box<[u64]>,
}

impl WorkerStorage {
    /// Allocates zero-initialized, correctly aligned storage for one managed heap.
    pub fn new(heap_size: usize) -> Result<Self, i32> {
        if heap_size == 0 {
            return Err(PARALLEL_TRANSFER_INVALID_ARGUMENT);
        }
        let arena_words = heap_size
            .checked_add(15)
            .map(|bytes| bytes / 16)
            .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
        Ok(Self {
            heap_size,
            arena: zeroed_boxed_slice(arena_words)?,
            object_index: zeroed_boxed_slice(object_handle_index_slots(heap_size))?,
            object_free: zeroed_boxed_slice(object_handle_free_slots(heap_size))?,
            buffer_registry: zeroed_boxed_slice(buffer_registry_bytes() / 8)?,
            serialize_object_ptrs: zeroed_boxed_slice(SERIALIZE_OBJECT_TABLE_SLOTS)?,
            serialize_object_indexes: zeroed_boxed_slice(SERIALIZE_OBJECT_TABLE_SLOTS)?,
            unserialize_values: zeroed_boxed_slice(UNSERIALIZE_VALUE_TABLE_SLOTS)?,
        })
    }

    /// Returns the requested heap byte limit, excluding alignment padding in the arena allocation.
    pub fn heap_size(&self) -> usize {
        self.heap_size
    }

    /// Borrows the 16-byte-aligned worker arena for the context's managed allocations.
    pub fn arena_ptr(&mut self) -> *mut u8 {
        self.arena.as_mut_ptr().cast::<u8>()
    }

    /// Borrows the worker-local object-handle index table consumed by context acquisition.
    pub fn object_index_ptr(&mut self) -> *mut u32 {
        self.object_index.as_mut_ptr()
    }

    /// Borrows the worker-local free-object-handle stack consumed by context acquisition.
    pub fn object_free_ptr(&mut self) -> *mut u32 {
        self.object_free.as_mut_ptr()
    }

    /// Borrows the worker-local native-buffer registry in the shared context layout.
    pub fn buffer_registry_ptr(&mut self) -> *mut u8 {
        self.buffer_registry.as_mut_ptr().cast::<u8>()
    }

    /// Borrows the serialization identity table without sharing parent-context object pointers.
    pub fn serialize_object_ptrs_ptr(&mut self) -> *mut u64 {
        self.serialize_object_ptrs.as_mut_ptr()
    }

    /// Borrows the matching worker-local serialization object-index table.
    pub fn serialize_object_indexes_ptr(&mut self) -> *mut u64 {
        self.serialize_object_indexes.as_mut_ptr()
    }

    /// Borrows the worker-local unserialization value table for local reconstruction.
    pub fn unserialize_values_ptr(&mut self) -> *mut u64 {
        self.unserialize_values.as_mut_ptr()
    }
}

/// Allocates a default-initialized worker table with a checked capacity reservation.
fn zeroed_boxed_slice<T: Default + Clone>(len: usize) -> Result<Box<[T]>, i32> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    values.resize(len, T::default());
    Ok(values.into_boxed_slice())
}

/// Runs one callback with a freshly acquired isolated context on the calling OS thread.
///
/// # Safety
/// Nonzero addresses must name the compiler-owned acquire/release/entry/callback functions
/// with the exact ABI types above. The job and code must remain valid through execution;
/// the native entry must obey the context-release-status handoff contract.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_run_worker_v1(
    acquire_addr: usize,
    release_addr: usize,
    entry_addr: usize,
    callback_addr: usize,
    job: *mut c_void,
    heap_size: usize,
    stack_size: usize,
) -> i64 {
    match catch_unwind(AssertUnwindSafe(|| {
        run_worker_v1(
            acquire_addr,
            release_addr,
            entry_addr,
            callback_addr,
            job,
            heap_size,
            stack_size,
        )
    })) {
        Ok(status) => status,
        Err(_) => PARALLEL_WORKER_PANIC,
    }
}

/// Validates worker settings, owns its backing storage and releases or quarantines the context once.
pub(crate) fn run_worker_v1(
    acquire_addr: usize,
    release_addr: usize,
    entry_addr: usize,
    callback_addr: usize,
    job: *mut c_void,
    heap_size: usize,
    stack_size: usize,
) -> i64 {
    if acquire_addr == 0
        || release_addr == 0
        || entry_addr == 0
        || callback_addr == 0
        || heap_size == 0
        || !(WORKER_STACK_MIN_BYTES..=WORKER_STACK_MAX_BYTES).contains(&stack_size)
    {
        return PARALLEL_WORKER_INVALID_ARGUMENT;
    }
    let mut storage = match WorkerStorage::new(heap_size) {
        Ok(storage) => storage,
        Err(_) => return PARALLEL_WORKER_ALLOCATION_FAILED,
    };
    // Safety: addresses originate from compiler-emitted runtime labels in the same binary.
    let acquire = unsafe { std::mem::transmute::<usize, ContextAcquire>(acquire_addr) };
    let release = unsafe { std::mem::transmute::<usize, ContextRelease>(release_addr) };
    let entry = unsafe { std::mem::transmute::<usize, WorkerEntry>(entry_addr) };
    let context = unsafe {
        acquire(
            storage.arena_ptr(),
            storage.heap_size(),
            storage.object_index_ptr(),
            storage.object_free_ptr(),
            storage.buffer_registry_ptr(),
            storage.serialize_object_ptrs_ptr(),
            storage.serialize_object_indexes_ptr(),
            storage.unserialize_values_ptr(),
        )
    };
    if context.is_null() {
        return PARALLEL_WORKER_CONTEXT_UNAVAILABLE;
    }
    struct ReleaseGuard {
        context: *mut c_void,
        release: ContextRelease,
        storage: Option<WorkerStorage>,
        finished: bool,
    }
    impl ReleaseGuard {
        /// Applies an entry-reported release status, preserving dirty backing instead of freeing it.
        fn finish_with_status(&mut self, status: i64) -> i64 {
            if self.finished {
                return 1;
            }
            self.finished = true;
            if status == 1 {
                self.storage.take();
            } else if let Some(storage) = self.storage.take() {
                std::mem::forget(storage);
            }
            status
        }

        /// Invokes fallback context release only when entry has not already supplied its status.
        fn finish(&mut self) -> i64 {
            if self.finished {
                return 1;
            }
            let status = unsafe { (self.release)(self.context) };
            self.finish_with_status(status)
        }
    }
    impl Drop for ReleaseGuard {
        /// Attempts context release during unwinding while retaining backing on cleanup failure.
        fn drop(&mut self) {
            self.finish();
        }
    }
    let mut release_guard = ReleaseGuard {
        context,
        release,
        storage: Some(storage),
        finished: false,
    };
    let _worker = ParallelWorkerGuard::enter();
    #[cfg(test)]
    if callback_addr == TEST_PANIC_CALLBACK_ADDR {
        panic!("controlled Parallel worker bridge panic");
    }
    // Safety: the callback address originates from compiler-emitted code in the same binary.
    let callback = unsafe { std::mem::transmute::<usize, WorkerCallback>(callback_addr) };
    let mut worker_release_status = WORKER_ENTRY_RELEASE_NOT_ATTEMPTED;
    let status = unsafe { entry(context, callback, job, stack_size, &mut worker_release_status) };
    let release_status = if worker_release_status == WORKER_ENTRY_RELEASE_NOT_ATTEMPTED {
        release_guard.finish()
    } else {
        release_guard.finish_with_status(worker_release_status)
    };
    if release_status == PARALLEL_WORKER_PHP_FATAL {
        return PARALLEL_WORKER_PHP_FATAL;
    }
    if release_status != 1 {
        // A callback-level PHP fatal remains the useful worker outcome when context cleanup
        // subsequently discovers dirty state and quarantines its backing storage.
        if status == PARALLEL_WORKER_PHP_FATAL {
            return status;
        }
        return PARALLEL_WORKER_CLEANUP_FAILED;
    }
    status
}

/// Copies one serialized PHP payload into native, versioned worker-owned storage.
///
/// # Safety
/// A nonempty source must be readable for its declared length. A nonnull output must be
/// writable for one blob and disjoint from the source; successful output is caller-owned.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_transfer_copy(
    source: *const u8,
    source_len: usize,
    output: *mut ParallelTransferBlob,
) -> i32 {
    if output.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    output.write(ParallelTransferBlob::EMPTY);
    if source.is_null() && source_len != 0 {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    match catch_unwind(AssertUnwindSafe(|| copy_transfer_payload(source, source_len))) {
        Ok(Ok(blob)) => {
            output.write(blob);
            PARALLEL_TRANSFER_OK
        }
        Ok(Err(status)) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Copies a caller-validated byte range behind the versioned header using checked allocation sizing.
pub(crate) fn copy_transfer_payload(
    source: *const u8,
    source_len: usize,
) -> Result<ParallelTransferBlob, i32> {
    let total = TRANSFER_MAGIC
        .len()
        .checked_add(source_len)
        .filter(|total| *total <= isize::MAX as usize)
        .ok_or(PARALLEL_TRANSFER_INVALID_ARGUMENT)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(total)
        .map_err(|_| PARALLEL_TRANSFER_ALLOCATION_FAILED)?;
    bytes.extend_from_slice(&TRANSFER_MAGIC);
    if source_len != 0 {
        // Safety: the C ABI requires `source` to reference `source_len` readable bytes.
        bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(source, source_len) });
    }
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed).cast::<u8>();
    Ok(ParallelTransferBlob { ptr, len })
}

/// Validates a transfer envelope and returns a borrowed view of its serialized payload.
///
/// # Safety
/// A nonnull blob pointer must be readable for its declared length. Nonnull output pointers
/// must be writable, mutually disjoint and disjoint from the input. The returned view expires
/// when the blob is released or modified and does not acquire ownership.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_transfer_payload(
    blob: ParallelTransferBlob,
    payload_ptr: *mut *const u8,
    payload_len: *mut usize,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        transfer_payload_view(blob, payload_ptr, payload_len)
    })) {
        Ok(status) => status,
        Err(_) => PARALLEL_TRANSFER_PANIC,
    }
}

/// Clears caller-validated output slots, then exposes bytes only after the native header matches.
fn transfer_payload_view(
    blob: ParallelTransferBlob,
    payload_ptr: *mut *const u8,
    payload_len: *mut usize,
) -> i32 {
    if payload_ptr.is_null() || payload_len.is_null() {
        return PARALLEL_TRANSFER_INVALID_ARGUMENT;
    }
    unsafe {
        payload_ptr.write(ptr::null());
        payload_len.write(0);
    }
    if !transfer_blob_is_valid(blob) {
        return PARALLEL_TRANSFER_INVALID_ENVELOPE;
    }
    unsafe {
        payload_ptr.write(blob.ptr.add(TRANSFER_MAGIC.len()));
        payload_len.write(blob.len - TRANSFER_MAGIC.len());
    }
    PARALLEL_TRANSFER_OK
}

/// Releases one blob previously returned by `elephc_parallel_transfer_copy`.
///
/// # Safety
/// A nonnull blob must retain its original allocation pointer and length and be released
/// exactly once, after every borrowed payload view has expired.
#[no_mangle]
pub unsafe extern "C" fn elephc_parallel_transfer_free(blob: ParallelTransferBlob) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        unsafe { release_transfer_blob(blob) };
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    static WORKER_TEST_LOCK: Mutex<()> = Mutex::new(());
    static ACQUIRE_VALID: AtomicUsize = AtomicUsize::new(0);
    static RELEASED_CONTEXT: AtomicUsize = AtomicUsize::new(0);
    static RELEASE_CALLS: AtomicUsize = AtomicUsize::new(0);

    /// Checks supplied table pointers and alignment, returning a non-dereferenced context sentinel.
    unsafe extern "C" fn acquire_context(
        arena: *mut u8,
        heap_size: usize,
        object_index: *mut u32,
        object_free: *mut u32,
        buffer_registry: *mut u8,
        serialize_object_ptrs: *mut u64,
        serialize_object_indexes: *mut u64,
        unserialize_values: *mut u64,
    ) -> *mut c_void {
        let valid = !arena.is_null()
            && arena as usize % 16 == 0
            && heap_size == 4096
            && !object_index.is_null()
            && !object_free.is_null()
            && !buffer_registry.is_null()
            && !serialize_object_ptrs.is_null()
            && !serialize_object_indexes.is_null()
            && !unserialize_values.is_null();
        ACQUIRE_VALID.store(usize::from(valid), Ordering::Relaxed);
        if valid {
            0x1000usize as *mut c_void
        } else {
            ptr::null_mut()
        }
    }

    /// Simulates pool exhaustion without accepting ownership of the supplied worker tables.
    unsafe extern "C" fn unavailable_context(
        _arena: *mut u8,
        _heap_size: usize,
        _object_index: *mut u32,
        _object_free: *mut u32,
        _buffer_registry: *mut u8,
        _serialize_object_ptrs: *mut u64,
        _serialize_object_indexes: *mut u64,
        _unserialize_values: *mut u64,
    ) -> *mut c_void {
        ptr::null_mut()
    }

    /// Counts successful mock releases without dereferencing the context sentinel.
    unsafe extern "C" fn release_context(context: *mut c_void) -> i64 {
        RELEASE_CALLS.fetch_add(1, Ordering::Relaxed);
        RELEASED_CONTEXT.store(context as usize, Ordering::Relaxed);
        1
    }

    /// Simulates a dirty context whose backing must remain quarantined after cleanup refusal.
    unsafe extern "C" fn reject_dirty_context(context: *mut c_void) -> i64 {
        RELEASED_CONTEXT.store(context as usize, Ordering::Relaxed);
        0
    }

    /// Checks mock entry settings and leaves release-status reporting to the bridge fallback.
    unsafe extern "C" fn worker_entry(
        context: *mut c_void,
        callback: WorkerCallback,
        job: *mut c_void,
        stack_size: usize,
        _release_status: *mut i64,
    ) -> i64 {
        if context as usize != 0x1000
            || stack_size != elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES
        {
            return -99;
        }
        unsafe { callback(job) }
    }

    /// Reports a mock release through the out-parameter to exercise double-release prevention.
    unsafe extern "C" fn worker_entry_releases_context(
        context: *mut c_void,
        callback: WorkerCallback,
        job: *mut c_void,
        stack_size: usize,
        release_status: *mut i64,
    ) -> i64 {
        if context as usize != 0x1000
            || stack_size != elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES
        {
            return -99;
        }
        unsafe { release_status.write(release_context(context)) };
        unsafe { callback(job) }
    }

    /// Reads the live test i64 job payload as the mock worker's return value.
    unsafe extern "C" fn worker_callback(job: *mut c_void) -> i64 {
        if job.is_null() {
            return -98;
        }
        unsafe { *(job.cast::<i64>()) }
    }

    /// Samples the bridge's thread-local worker flag from inside the isolated callback boundary.
    unsafe extern "C" fn worker_active_callback(_job: *mut c_void) -> i64 {
        i64::from(elephc_parallel_worker_active())
    }

    #[test]
    /// Checks versioned native copying, borrowed payload access and explicit allocation release.
    fn payload_round_trips_through_a_versioned_native_blob() {
        let source = b"a:2:{i:0;i:42;i:1;s:2:\"ok\";}";
        let mut blob = ParallelTransferBlob::EMPTY;
        assert_eq!(
            unsafe { elephc_parallel_transfer_copy(source.as_ptr(), source.len(), &mut blob) },
            PARALLEL_TRANSFER_OK
        );
        assert!(!blob.ptr.is_null());
        assert_eq!(blob.len, TRANSFER_MAGIC.len() + source.len());

        let mut payload = ptr::null();
        let mut payload_len = 0;
        assert_eq!(
            unsafe { elephc_parallel_transfer_payload(blob, &mut payload, &mut payload_len) },
            PARALLEL_TRANSFER_OK
        );
        assert_eq!(unsafe { std::slice::from_raw_parts(payload, payload_len) }, source);
        unsafe { elephc_parallel_transfer_free(blob) };
    }

    #[test]
    /// Distinguishes a valid empty allocation from a null source with a nonzero declared length.
    fn empty_payload_is_distinct_from_invalid_input() {
        let mut blob = ParallelTransferBlob::EMPTY;
        assert_eq!(
            unsafe { elephc_parallel_transfer_copy(ptr::null(), 0, &mut blob) },
            PARALLEL_TRANSFER_OK
        );
        let mut payload = ptr::null();
        let mut payload_len = usize::MAX;
        assert_eq!(
            unsafe { elephc_parallel_transfer_payload(blob, &mut payload, &mut payload_len) },
            PARALLEL_TRANSFER_OK
        );
        assert_eq!(payload_len, 0);
        unsafe { elephc_parallel_transfer_free(blob) };

        assert_eq!(
            unsafe { elephc_parallel_transfer_copy(ptr::null(), 1, &mut blob) },
            PARALLEL_TRANSFER_INVALID_ARGUMENT
        );
        assert!(blob.ptr.is_null());
    }

    #[test]
    /// Rejects a wrong envelope header and clears both borrowed-view output slots.
    fn malformed_envelopes_fail_closed_and_clear_outputs() {
        let mut bytes = *b"NOT-EPAR";
        let blob = ParallelTransferBlob {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
        let mut payload = bytes.as_ptr();
        let mut payload_len = usize::MAX;
        assert_eq!(
            unsafe { elephc_parallel_transfer_payload(blob, &mut payload, &mut payload_len) },
            PARALLEL_TRANSFER_INVALID_ENVELOPE
        );
        assert!(payload.is_null());
        assert_eq!(payload_len, 0);
    }

    #[test]
    /// Transfers native allocation ownership to another OS thread for decoding and release.
    fn blob_ownership_can_move_to_another_thread() {
        let source = b"s:6:\"worker\";";
        let mut blob = ParallelTransferBlob::EMPTY;
        assert_eq!(
            unsafe { elephc_parallel_transfer_copy(source.as_ptr(), source.len(), &mut blob) },
            PARALLEL_TRANSFER_OK
        );
        let ptr = blob.ptr as usize;
        let len = blob.len;
        let decoded = std::thread::spawn(move || {
            let blob = ParallelTransferBlob {
                ptr: ptr as *mut u8,
                len,
            };
            let mut payload = ptr::null();
            let mut payload_len = 0;
            let status = unsafe {
                elephc_parallel_transfer_payload(blob, &mut payload, &mut payload_len)
            };
            let copied = if status == PARALLEL_TRANSFER_OK {
                unsafe { std::slice::from_raw_parts(payload, payload_len) }.to_vec()
            } else {
                Vec::new()
            };
            unsafe { elephc_parallel_transfer_free(blob) };
            (status, copied)
        })
        .join()
        .expect("worker must not panic");
        assert_eq!(decoded, (PARALLEL_TRANSFER_OK, source.to_vec()));
    }

    #[test]
    /// Checks arena alignment and every zero-initialized table against the shared layout sizing.
    fn worker_storage_matches_the_shared_layout_and_alignment() {
        let heap_size = 4097;
        let mut storage = WorkerStorage::new(heap_size).expect("worker storage");
        assert_eq!(storage.heap_size(), heap_size);
        assert_eq!(storage.arena_ptr() as usize % 16, 0);
        assert_eq!(storage.object_index.len(), object_handle_index_slots(heap_size));
        assert_eq!(storage.object_free.len(), object_handle_free_slots(heap_size));
        assert_eq!(storage.buffer_registry.len() * 8, buffer_registry_bytes());
        assert_eq!(storage.serialize_object_ptrs.len(), SERIALIZE_OBJECT_TABLE_SLOTS);
        assert_eq!(storage.serialize_object_indexes.len(), SERIALIZE_OBJECT_TABLE_SLOTS);
        assert_eq!(storage.unserialize_values.len(), UNSERIALIZE_VALUE_TABLE_SLOTS);
        assert!(storage.arena.iter().all(|word| *word == 0));
        assert!(storage.object_index.iter().all(|word| *word == 0));
        assert!(storage.object_free.iter().all(|word| *word == 0));
        assert!(storage.buffer_registry.iter().all(|word| *word == 0));
        assert!(storage.serialize_object_ptrs.iter().all(|word| *word == 0));
        assert!(storage.serialize_object_indexes.iter().all(|word| *word == 0));
        assert!(storage.unserialize_values.iter().all(|word| *word == 0));
    }

    #[test]
    /// Confirms worker backing storage can be owned and dropped by another OS thread.
    fn worker_storage_ownership_moves_to_a_thread() {
        let storage = WorkerStorage::new(4096).expect("worker storage");
        let heap_size = std::thread::spawn(move || storage.heap_size())
            .join()
            .expect("worker storage move");
        assert_eq!(heap_size, 4096);
    }

    #[test]
    /// Exercises mock acquisition, callback execution and fallback release of one context.
    fn run_worker_acquires_enters_and_releases_one_context() {
        let _guard = WORKER_TEST_LOCK.lock().unwrap();
        ACQUIRE_VALID.store(0, Ordering::Relaxed);
        RELEASED_CONTEXT.store(0, Ordering::Relaxed);
        let mut expected = 42i64;
        let status = unsafe {
            elephc_parallel_run_worker_v1(
                    acquire_context as *const () as usize,
                release_context as *const () as usize,
                worker_entry as *const () as usize,
                worker_callback as *const () as usize,
                (&mut expected as *mut i64).cast(),
                4096,
                elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES,
            )
        };
        assert_eq!(status, 42);
        assert_eq!(ACQUIRE_VALID.load(Ordering::Relaxed), 1);
        assert_eq!(RELEASED_CONTEXT.load(Ordering::Relaxed), 0x1000);
    }

    #[test]
    /// Ensures an entry-reported release prevents a second release from the bridge guard.
    fn worker_entry_release_status_prevents_a_second_context_release() {
        let _guard = WORKER_TEST_LOCK.lock().unwrap();
        RELEASE_CALLS.store(0, Ordering::Relaxed);
        RELEASED_CONTEXT.store(0, Ordering::Relaxed);
        let mut expected = 42i64;
        let status = unsafe {
            elephc_parallel_run_worker_v1(
                acquire_context as *const () as usize,
                release_context as *const () as usize,
                worker_entry_releases_context as *const () as usize,
                worker_callback as *const () as usize,
                (&mut expected as *mut i64).cast(),
                4096,
                elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES,
            )
        };
        assert_eq!(status, 42);
        assert_eq!(RELEASED_CONTEXT.load(Ordering::Relaxed), 0x1000);
        assert_eq!(RELEASE_CALLS.load(Ordering::Relaxed), 1);
    }

    #[test]
    /// Maps refused mock cleanup to a typed failure while preserving dirty context backing.
    fn run_worker_reports_cleanup_failure_and_preserves_dirty_backing() {
        let _guard = WORKER_TEST_LOCK.lock().unwrap();
        RELEASED_CONTEXT.store(0, Ordering::Relaxed);
        let mut expected = 42i64;
        let status = unsafe {
            elephc_parallel_run_worker_v1(
                acquire_context as *const () as usize,
                reject_dirty_context as *const () as usize,
                worker_entry as *const () as usize,
                worker_callback as *const () as usize,
                (&mut expected as *mut i64).cast(),
                4096,
                elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES,
            )
        };
        assert_eq!(status, PARALLEL_WORKER_CLEANUP_FAILED);
        assert_eq!(RELEASED_CONTEXT.load(Ordering::Relaxed), 0x1000);
    }

    #[test]
    /// Checks the worker flag is active inside the callback and cleared after its return.
    fn worker_active_flag_is_scoped_to_the_isolated_callback() {
        let _guard = WORKER_TEST_LOCK.lock().unwrap();
        assert_eq!(elephc_parallel_worker_active(), 0);
        let status = unsafe {
            elephc_parallel_run_worker_v1(
                acquire_context as *const () as usize,
                release_context as *const () as usize,
                worker_entry as *const () as usize,
                worker_active_callback as *const () as usize,
                ptr::null_mut(),
                4096,
                elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES,
            )
        };
        assert_eq!(status, 1);
        assert_eq!(elephc_parallel_worker_active(), 0);
    }

    #[test]
    /// Checks parent-scope ownership is non-reentrant, thread-local and explicitly cleared.
    fn parent_scope_flag_is_thread_local_non_reentrant_and_clears() {
        assert_eq!(elephc_parallel_parent_scope_active(), 0);
        assert_eq!(elephc_parallel_parent_scope_enter(), 1);
        assert_eq!(elephc_parallel_parent_scope_active(), 1);
        assert_eq!(elephc_parallel_parent_scope_enter(), 0);

        let child_active = std::thread::spawn(|| elephc_parallel_parent_scope_active())
            .join()
            .expect("scope query must not panic on another thread");
        assert_eq!(child_active, 0);

        assert_eq!(elephc_parallel_parent_scope_leave(), 1);
        assert_eq!(elephc_parallel_parent_scope_active(), 0);
        assert_eq!(elephc_parallel_parent_scope_leave(), 0);
    }

    #[test]
    /// Rejects missing entry addresses and exhausted context acquisition without attempting release.
    fn run_worker_reports_invalid_or_exhausted_context_without_entering() {
        let _guard = WORKER_TEST_LOCK.lock().unwrap();
        RELEASED_CONTEXT.store(0, Ordering::Relaxed);
        assert_eq!(
            unsafe {
                elephc_parallel_run_worker_v1(
                    0,
                    release_context as *const () as usize,
                    worker_entry as *const () as usize,
                    worker_callback as *const () as usize,
                    ptr::null_mut(),
                    4096,
                    elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES,
                )
            },
            PARALLEL_WORKER_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe {
                elephc_parallel_run_worker_v1(
                    unavailable_context as *const () as usize,
                    release_context as *const () as usize,
                    worker_entry as *const () as usize,
                    worker_callback as *const () as usize,
                    ptr::null_mut(),
                    4096,
                    elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES,
                )
            },
            PARALLEL_WORKER_CONTEXT_UNAVAILABLE
        );
        assert_eq!(RELEASED_CONTEXT.load(Ordering::Relaxed), 0);
    }
}
