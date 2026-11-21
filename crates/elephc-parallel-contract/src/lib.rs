//! Purpose:
//! Defines the worker side-table layout shared by Elephc runtime emission and the Parallel bridge.
//!
//! Called from:
//! - Core runtime data sizing and Buffer/object handle emitters.
//! - `elephc-parallel` worker storage allocation.
//!
//! Key details:
//! - Object tables scale with the configured managed heap size.
//! - Buffer descriptors have a fixed generation-safe capacity and byte stride.

pub const BUFFER_REGISTRY_CAPACITY: usize = 4096;
pub const BUFFER_DESCRIPTOR_SIZE: usize = 48;
pub const WORKER_STACK_MIN_BYTES: usize = 256 * 1024;
pub const WORKER_STACK_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const WORKER_STACK_DEFAULT_BYTES: usize = 8 * 1024 * 1024;
pub const PARALLEL_WORKER_PHP_FATAL: i64 = -5;
pub const PARALLEL_WORKER_TRANSFER_DECODE: i64 = -6;
pub const PARALLEL_WORKER_TRANSFER_ENCODE: i64 = -7;
pub const PARALLEL_WORKER_ARENA_EXHAUSTED: i64 = -8;
pub const PARALLEL_WORKER_CLEANUP_FAILED: i64 = i64::MIN + 5;
pub const PARALLEL_TRANSFER_ALLOCATION_FAILED: i32 = 2;
pub const RUNTIME_CONTEXT_POOL_SLOTS: usize = 8;
pub const PARALLEL_WORKER_CONTEXT_CAPACITY: usize = RUNTIME_CONTEXT_POOL_SLOTS - 1;
pub const SERIALIZE_OBJECT_TABLE_SLOTS: usize = 65_536;
pub const UNSERIALIZE_VALUE_TABLE_SLOTS: usize = 65_536;

/// Sizes the granule-to-object-handle index, including the runtime's two trailing slack slots.
pub const fn object_handle_index_slots(heap_size: usize) -> usize {
    heap_size / 16 + 2
}

/// Sizes the released-object-handle stack with the runtime's sixteen-slot safety margin.
pub const fn object_handle_free_slots(heap_size: usize) -> usize {
    heap_size / 24 + 16
}

/// Returns storage for all generation-safe Buffer descriptors, including reserved slot zero.
pub const fn buffer_registry_bytes() -> usize {
    (BUFFER_REGISTRY_CAPACITY + 1) * BUFFER_DESCRIPTOR_SIZE
}

/// Sizes the live-payload-base bitmap: one bit per 8-byte heap granule, at least one byte.
pub const fn heap_live_bases_bytes(heap_size: usize) -> usize {
    let bytes = heap_size.div_ceil(64);
    if bytes == 0 { 1 } else { bytes }
}

/// Sizes the bin-mark table: one byte per 16-byte heap granule plus two trailing slack bytes.
pub const fn heap_bin_mark_bytes(heap_size: usize) -> usize {
    heap_size / 16 + 2
}

/// Sizes the WeakReference index: one 8-byte cell per object-handle index slot.
pub const fn weak_reference_index_slots(heap_size: usize) -> usize {
    object_handle_index_slots(heap_size)
}

/// Storage a pooled runtime context borrows from its owner for one managed heap.
///
/// `__rt_ctx_acquire` receives ONE pointer to this record instead of one argument per table,
/// so adding a table never changes the calling convention again. Every pointer must be
/// non-null, zero-initialized and outlive the context. The field order is the ABI: the runtime
/// emitter reads each field at the `CONTEXT_STORAGE_*` offset below.
#[repr(C)]
pub struct ContextStorage {
    pub arena: *mut u8,
    pub heap_size: usize,
    pub object_index: *mut u32,
    pub object_free: *mut u32,
    pub buffer_registry: *mut u8,
    pub serialize_object_ptrs: *mut u64,
    pub serialize_object_indexes: *mut u64,
    pub unserialize_values: *mut u64,
    pub heap_live_bases: *mut u8,
    pub heap_bin_marks: *mut u8,
    pub weak_reference_index: *mut u64,
}

pub const CONTEXT_STORAGE_ARENA: usize = std::mem::offset_of!(ContextStorage, arena);
pub const CONTEXT_STORAGE_HEAP_SIZE: usize = std::mem::offset_of!(ContextStorage, heap_size);
pub const CONTEXT_STORAGE_OBJECT_INDEX: usize = std::mem::offset_of!(ContextStorage, object_index);
pub const CONTEXT_STORAGE_OBJECT_FREE: usize = std::mem::offset_of!(ContextStorage, object_free);
pub const CONTEXT_STORAGE_BUFFER_REGISTRY: usize = std::mem::offset_of!(ContextStorage, buffer_registry);
pub const CONTEXT_STORAGE_SERIALIZE_OBJECT_PTRS: usize =
    std::mem::offset_of!(ContextStorage, serialize_object_ptrs);
pub const CONTEXT_STORAGE_SERIALIZE_OBJECT_INDEXES: usize =
    std::mem::offset_of!(ContextStorage, serialize_object_indexes);
pub const CONTEXT_STORAGE_UNSERIALIZE_VALUES: usize =
    std::mem::offset_of!(ContextStorage, unserialize_values);
pub const CONTEXT_STORAGE_HEAP_LIVE_BASES: usize = std::mem::offset_of!(ContextStorage, heap_live_bases);
pub const CONTEXT_STORAGE_HEAP_BIN_MARKS: usize = std::mem::offset_of!(ContextStorage, heap_bin_marks);
pub const CONTEXT_STORAGE_WEAK_REFERENCE_INDEX: usize =
    std::mem::offset_of!(ContextStorage, weak_reference_index);

#[cfg(test)]
mod tests {
    use super::*;

    /// Default eight-MiB worker storage agrees with runtime index/free-stack/Buffer sizing.
    #[test]
    fn default_heap_layout_matches_the_runtime_contract() {
        let heap = 8 * 1024 * 1024;
        assert_eq!(object_handle_index_slots(heap), 524_290);
        assert_eq!(object_handle_free_slots(heap), 349_541);
        assert_eq!(buffer_registry_bytes(), 196_656);
    }

    /// Small and non-aligned heaps retain the documented metadata slack without rounding errors.
    #[test]
    fn side_tables_keep_the_runtime_slack_beyond_the_heap_bound() {
        assert_eq!(object_handle_index_slots(1), 2);
        assert_eq!(object_handle_index_slots(16), 3);
        assert_eq!(object_handle_index_slots(17), 3);
        assert_eq!(object_handle_free_slots(25), 17);
    }

    /// Worker concurrency reserves one runtime context capacity slot for the main context.
    #[test]
    fn worker_capacity_excludes_the_main_context_slot() {
        assert_eq!(RUNTIME_CONTEXT_POOL_SLOTS, 8);
        assert_eq!(PARALLEL_WORKER_CONTEXT_CAPACITY, 7);
    }

    /// The acquire descriptor is a dense array of eight-byte words in declaration order.
    #[test]
    fn context_storage_fields_are_consecutive_words() {
        let offsets = [
            CONTEXT_STORAGE_ARENA, CONTEXT_STORAGE_HEAP_SIZE, CONTEXT_STORAGE_OBJECT_INDEX,
            CONTEXT_STORAGE_OBJECT_FREE, CONTEXT_STORAGE_BUFFER_REGISTRY,
            CONTEXT_STORAGE_SERIALIZE_OBJECT_PTRS, CONTEXT_STORAGE_SERIALIZE_OBJECT_INDEXES,
            CONTEXT_STORAGE_UNSERIALIZE_VALUES, CONTEXT_STORAGE_HEAP_LIVE_BASES,
            CONTEXT_STORAGE_HEAP_BIN_MARKS, CONTEXT_STORAGE_WEAK_REFERENCE_INDEX,
        ];
        for (index, offset) in offsets.into_iter().enumerate() {
            assert_eq!(offset, index * 8);
        }
        assert_eq!(std::mem::size_of::<ContextStorage>(), offsets.len() * 8);
    }

    /// Heap side tables follow the allocator's granule sizes for small and default heaps.
    #[test]
    fn heap_side_tables_scale_with_the_heap() {
        assert_eq!(heap_live_bases_bytes(0), 1);
        assert_eq!(heap_live_bases_bytes(65), 2);
        assert_eq!(heap_bin_mark_bytes(8 * 1024 * 1024), 524_290);
        assert_eq!(weak_reference_index_slots(16), 3);
    }

    /// Serialization and unserialization identity tables retain their shared byte capacities.
    #[test]
    fn serializer_object_tables_match_the_runtime_capacity() {
        assert_eq!(SERIALIZE_OBJECT_TABLE_SLOTS * 8, 524_288);
        assert_eq!(UNSERIALIZE_VALUE_TABLE_SLOTS * 8, 524_288);
    }
}
