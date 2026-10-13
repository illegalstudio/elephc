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

    /// Serialization and unserialization identity tables retain their shared byte capacities.
    #[test]
    fn serializer_object_tables_match_the_runtime_capacity() {
        assert_eq!(SERIALIZE_OBJECT_TABLE_SLOTS * 8, 524_288);
        assert_eq!(UNSERIALIZE_VALUE_TABLE_SLOTS * 8, 524_288);
    }
}
