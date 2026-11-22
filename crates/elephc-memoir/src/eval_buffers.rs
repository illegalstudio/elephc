//! Purpose:
//! Owns every eval `buffer_new()` allocation behind a generation-checked handle registry,
//! so a freed eval buffer fails closed exactly like a compiled `buffer<T>` handle.
//!
//! Called from:
//! - Magician’s `buffer_new`, `buffer_len`, `buffer_ptr`, and `buffer_free` builtins.
//!
//! Key details:
//! - A handle is `(generation << 32) | EVAL_BUFFER_INDEX_TAG | slot`. Generations stay in
//!   `1..=EVAL_BUFFER_MAX_GENERATION`, so every handle is a positive PHP int and 0 is never
//!   issued. The index tag keeps eval handles disjoint from compiled handles, whose low half
//!   is a descriptor index of at most 4096, so neither registry resolves the other's handles.
//! - Reusing a freed slot raises its generation, so an alias from an earlier lifetime never
//!   resolves again, even when the allocator hands the same payload address back. A slot that
//!   reaches the maximum generation is retired instead of wrapping.
//! - The registry is thread-local: a compiled program runs PHP and every eval fragment on one
//!   OS thread (fibers switch stacks, not threads), and parallel `cargo test` threads stay
//!   isolated. Borrows never outlive one bookkeeping step, so no user code runs inside them.
//! - Payloads are zeroed `calloc` blocks of `length * EVAL_BUFFER_STRIDE` bytes, freed only
//!   after their slot is unpublished.

use std::cell::RefCell;

/// Failure to allocate, resolve or retire an eval buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferError {
    /// The handle is null, invalid or from another registry or lifetime.
    InvalidHandle,
    /// The checked allocation or registry access could not complete.
    Unavailable,
}

/// Element stride of every eval buffer: eval has no element types, so each element is a word.
const EVAL_BUFFER_STRIDE: usize = 8;

/// Bit set in the low half of every eval handle; compiled handles keep it clear.
const EVAL_BUFFER_INDEX_TAG: u32 = 1 << 31;

/// Highest slot generation; it keeps bit 63 of a handle clear, so handles stay positive.
const EVAL_BUFFER_MAX_GENERATION: u32 = EVAL_BUFFER_INDEX_TAG - 1;

thread_local! {
    /// Eval buffers of the current thread, shared by every eval context and fragment.
    static EVAL_BUFFERS: RefCell<EvalBufferRegistry> =
        const { RefCell::new(EvalBufferRegistry::new()) };
}

/// The payload and element count of a live eval buffer, resolved from its handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalBufferView {
    pub payload: usize,
    pub length: usize,
}

/// One registry slot: a live payload, or the generation a freed slot reuses next.
#[derive(Clone, Copy)]
struct EvalBufferSlot {
    payload: usize,
    length: usize,
    generation: u32,
    live: bool,
}

/// Slot table plus the indices of freed slots that may be reused.
struct EvalBufferRegistry {
    slots: Vec<EvalBufferSlot>,
    free: Vec<u32>,
}

impl EvalBufferRegistry {
    /// Creates an empty registry; usable in a `const` thread-local initializer.
    const fn new() -> Self {
        Self { slots: Vec::new(), free: Vec::new() }
    }

    /// Publishes a payload in a recycled or new slot and answers its handle, or `None` when
    /// every slot index the handle layout can encode is taken.
    fn insert(&mut self, payload: usize, length: usize) -> Option<i64> {
        let (index, generation) = match self.free.pop() {
            Some(index) => {
                let slot = self.slots.get_mut(index as usize)?;
                let generation = slot.generation.checked_add(1)?;
                *slot = EvalBufferSlot { payload, length, generation, live: true };
                (index, generation)
            }
            None => {
                let index = u32::try_from(self.slots.len())
                    .ok()
                    .filter(|index| index & EVAL_BUFFER_INDEX_TAG == 0)?;
                self.slots.push(EvalBufferSlot { payload, length, generation: 1, live: true });
                (index, 1)
            }
        };
        Some(encode_handle(index, generation))
    }

    /// Answers the live buffer a handle names, or `None` for a freed, stale, or foreign handle.
    fn resolve(&self, handle: i64) -> Option<EvalBufferView> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get(index as usize)?;
        (slot.live && slot.generation == generation)
            .then_some(EvalBufferView { payload: slot.payload, length: slot.length })
    }

    /// Unpublishes the live buffer a handle names and answers it so the caller can release
    /// the payload; `None` when the handle does not resolve, leaving the registry unchanged.
    fn remove(&mut self, handle: i64) -> Option<EvalBufferView> {
        let view = self.resolve(handle)?;
        let (index, _) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index as usize)?;
        slot.live = false;
        slot.payload = 0;
        slot.length = 0;
        if slot.generation < EVAL_BUFFER_MAX_GENERATION {
            self.free.push(index);
        }
        Some(view)
    }
}

/// Packs a slot index and its generation into the positive integer eval code sees.
fn encode_handle(index: u32, generation: u32) -> i64 {
    let raw = (u64::from(generation) << 32) | u64::from(EVAL_BUFFER_INDEX_TAG | index);
    raw as i64
}

/// Splits a handle into its slot index and generation; `None` for 0, negative values, and
/// compiled handles, none of which carry the eval index tag and a non-zero generation.
fn decode_handle(handle: i64) -> Option<(u32, u32)> {
    let raw = u64::try_from(handle).ok()?;
    let generation = (raw >> 32) as u32;
    let low = raw as u32;
    if generation == 0 || low & EVAL_BUFFER_INDEX_TAG == 0 {
        return None;
    }
    Some((low & !EVAL_BUFFER_INDEX_TAG, generation))
}

/// Runs one bookkeeping step against this thread's registry; fails when the registry is
/// already borrowed or torn down, which no buffer builtin does on its own.
fn with_registry<T>(
    operation: impl FnOnce(&mut EvalBufferRegistry) -> T,
) -> Result<T, BufferError> {
    match EVAL_BUFFERS.try_with(|registry| {
        registry.try_borrow_mut().ok().map(|mut registry| operation(&mut *registry))
    }) {
        Ok(Some(result)) => Ok(result),
        _ => Err(BufferError::Unavailable),
    }
}

/// Allocates a zeroed payload of `length` word elements and answers its new handle.
pub fn allocate(length: usize) -> Result<i64, BufferError> {
    let payload_bytes = length
        .checked_mul(EVAL_BUFFER_STRIDE)
        .ok_or(BufferError::Unavailable)?;
    let payload = allocate_payload(payload_bytes.max(1));
    if payload.is_null() {
        return Err(BufferError::Unavailable);
    }
    match with_registry(|registry| registry.insert(payload as usize, length)) {
        Ok(Some(handle)) => Ok(handle),
        _ => {
            free_payload(payload as usize);
            Err(BufferError::Unavailable)
        }
    }
}

/// Borrows the current pointer and length, valid until the handle is released.
pub fn resolve(handle: i64) -> Result<EvalBufferView, BufferError> {
    with_registry(|registry| registry.resolve(handle))?.ok_or(BufferError::InvalidHandle)
}

/// Removes a live pointer before freeing it. A nulled local can be freed again.
pub fn release(handle: i64) -> Result<(), BufferError> {
    if handle == 0 {
        return Ok(());
    }
    let view = with_registry(|registry| registry.remove(handle))?.ok_or(BufferError::InvalidHandle)?;
    free_payload(view.payload);
    Ok(())
}

/// Requests a zeroed, nonempty allocation from the eval buffer allocator.
#[allow(unsafe_code)]
fn allocate_payload(bytes: usize) -> *mut libc::c_void {
    // SAFETY: calloc takes sizes only. The caller validates the returned pointer.
    unsafe { libc::calloc(bytes, 1) }
}

/// Retires a pointer once it has been removed from, or failed to enter, the registry.
#[allow(unsafe_code)]
fn free_payload(payload: usize) {
    // SAFETY: both callers own a live calloc allocation and have no published slot for it.
    unsafe { libc::free(payload as *mut libc::c_void) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies issued handles are positive, carry the eval index tag, and that the shapes
    /// compiled code issues or treats as invalid (0, negative, untagged) never resolve.
    #[test]
    fn handles_are_positive_and_disjoint_from_compiled_handles() {
        let mut registry = EvalBufferRegistry::new();

        let handle = registry.insert(0x1000, 4).expect("first slot");

        assert!(handle > 0);
        assert_ne!((handle as u64 as u32) & EVAL_BUFFER_INDEX_TAG, 0);
        assert_eq!(registry.resolve(handle), Some(EvalBufferView { payload: 0x1000, length: 4 }));
        let compiled_shape = (1_i64 << 32) | 1;
        for invalid in [0, -1, compiled_shape, i64::from(EVAL_BUFFER_INDEX_TAG), i64::MIN] {
            assert_eq!(registry.resolve(invalid), None, "handle {invalid:#x} must not resolve");
        }
    }

    /// Verifies a freed slot that is reused, even for the same payload address, gets a new
    /// generation: the old handle neither resolves nor frees the new lifetime.
    #[test]
    fn reused_slot_rejects_handles_from_earlier_lifetimes() {
        let mut registry = EvalBufferRegistry::new();
        let stale = registry.insert(0x2000, 2).expect("first lifetime");

        assert_eq!(registry.remove(stale), Some(EvalBufferView { payload: 0x2000, length: 2 }));
        let fresh = registry.insert(0x2000, 3).expect("second lifetime");

        assert_ne!(stale, fresh);
        assert_eq!(
            decode_handle(stale).map(|(index, _)| index),
            decode_handle(fresh).map(|(index, _)| index)
        );
        assert_eq!(registry.resolve(stale), None);
        assert_eq!(registry.remove(stale), None);
        assert_eq!(registry.resolve(fresh), Some(EvalBufferView { payload: 0x2000, length: 3 }));
    }

    /// Verifies a second removal of the same handle fails instead of handing the payload out
    /// twice, and that other live buffers are untouched by it.
    #[test]
    fn removed_handle_cannot_be_removed_again() {
        let mut registry = EvalBufferRegistry::new();
        let kept = registry.insert(0x3000, 1).expect("kept buffer");
        let freed = registry.insert(0x4000, 5).expect("freed buffer");

        assert_eq!(registry.remove(freed), Some(EvalBufferView { payload: 0x4000, length: 5 }));

        assert_eq!(registry.remove(freed), None);
        assert_eq!(registry.resolve(kept), Some(EvalBufferView { payload: 0x3000, length: 1 }));
    }

    /// Verifies a slot freed at the maximum generation is retired: it never re-enters the free
    /// list, so the next buffer takes a new slot instead of wrapping the generation.
    #[test]
    fn slot_at_maximum_generation_is_retired() {
        let mut registry = EvalBufferRegistry::new();
        let first = registry.insert(0x5000, 1).expect("first slot");
        registry.slots[0].generation = EVAL_BUFFER_MAX_GENERATION;
        let last_lifetime = encode_handle(0, EVAL_BUFFER_MAX_GENERATION);

        assert_eq!(registry.resolve(first), None);
        assert!(registry.remove(last_lifetime).is_some());
        let next = registry.insert(0x6000, 1).expect("new slot");

        assert!(last_lifetime > 0);
        assert_eq!(decode_handle(next), Some((1, 1)));
        assert_eq!(registry.resolve(last_lifetime), None);
    }
}
