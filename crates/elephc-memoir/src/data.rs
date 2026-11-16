//! Purpose:
//! Emits heap, GC, scratch and buffer declarations in compiler-selected object syntax.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

/// Target syntax services needed by fixed memory data.
pub trait DataTarget {
    /// Emits one ordinary common-symbol declaration.
    fn common(&self, name: &str, bytes: usize) -> String;
    /// Emits a common symbol with an explicit payload alignment.
    fn aligned_common(&self, name: &str, bytes: usize, alignment: usize) -> String;
}

/// Appends labels used when reporting individual live heap blocks.
pub fn append_heap_debug_block_labels(out: &mut String) {
    out.push_str(".globl _heap_dbg_block_offset\n_heap_dbg_block_offset:\n    .ascii \"HEAP DEBUG: live offset=\"\n");
    out.push_str(".globl _heap_dbg_block_bytes\n_heap_dbg_block_bytes:\n    .ascii \" bytes=\"\n");
    out.push_str(".globl _heap_dbg_block_kind\n_heap_dbg_block_kind:\n    .ascii \" kind=\"\n");
    out.push_str(".globl _heap_dbg_block_refs\n_heap_dbg_block_refs:\n    .ascii \" refs=\"\n");
    out.push_str(".globl _heap_dbg_block_class\n_heap_dbg_block_class:\n    .ascii \" class_id=\"\n");
    out.push_str(".globl _heap_dbg_block_class_name\n_heap_dbg_block_class_name:\n    .ascii \" class=\"\n");
    out.push_str(".globl _heap_dbg_block_tag\n_heap_dbg_block_tag:\n    .ascii \" tag=\"\n");
    out.push_str(".globl _heap_dbg_block_length\n_heap_dbg_block_length:\n    .ascii \" length=\"\n");
    out.push_str(".globl _heap_dbg_block_fingerprint\n_heap_dbg_block_fingerprint:\n    .ascii \" fingerprint=\"\n");
}

/// Appends labels used by allocator statistics and leak reports.
pub fn append_heap_debug_stat_labels(out: &mut String) {
    out.push_str(".globl _heap_dbg_stats_prefix\n_heap_dbg_stats_prefix:\n    .ascii \"HEAP DEBUG: allocs=\"\n");
    out.push_str(".globl _heap_dbg_frees_label\n_heap_dbg_frees_label:\n    .ascii \" frees=\"\n");
    out.push_str(".globl _heap_dbg_live_blocks_label\n_heap_dbg_live_blocks_label:\n    .ascii \" live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_live_bytes_label\n_heap_dbg_live_bytes_label:\n    .ascii \" live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_peak_label\n_heap_dbg_peak_label:\n    .ascii \" peak_live_bytes=\"\n");
    out.push_str(".globl _heap_dbg_leak_prefix\n_heap_dbg_leak_prefix:\n    .ascii \"HEAP DEBUG: leak summary: \"\n");
    out.push_str(".globl _heap_dbg_live_blocks_short_label\n_heap_dbg_live_blocks_short_label:\n    .ascii \"live_blocks=\"\n");
    out.push_str(".globl _heap_dbg_clean_label\n_heap_dbg_clean_label:\n    .ascii \"clean\\n\"\n");
    out.push_str(".globl _heap_dbg_newline\n_heap_dbg_newline:\n    .ascii \"\\n\"\n");
}

/// Reserves allocation, free, live-byte and peak-byte counters.
pub fn append_memory_counters(out: &mut String, target: &impl DataTarget) {
    out.push_str(&target.common("_gc_allocs", 8));
    out.push_str(&target.common("_gc_frees", 8));
    out.push_str(&target.common("_gc_live", 8));
    out.push_str(&target.common("_gc_peak", 8));
}

/// Appends allocation-overflow and string-capacity failure messages.
pub fn append_allocation_messages(out: &mut String) {
    out.push_str(&format!(
        ".globl _alloc_overflow_msg\n_alloc_overflow_msg:\n    .ascii {:?}\n",
        crate::diagnostics::ALLOC_OVERFLOW_MSG
    ));
    out.push_str(&format!(
        ".globl _concat_overflow_msg\n_concat_overflow_msg:\n    .ascii {:?}\n",
        crate::diagnostics::CONCAT_OVERFLOW_MSG
    ));
}

/// Appends buffer allocation, bounds, lifetime and registry failures.
pub fn append_buffer_messages(out: &mut String) {
    out.push_str(&format!(
        ".globl _buffer_alloc_size_msg\n_buffer_alloc_size_msg:\n    .ascii {:?}\n",
        crate::diagnostics::BUFFER_ALLOC_SIZE_MSG
    ));
    out.push_str(".globl _buffer_bounds_msg\n_buffer_bounds_msg:\n    .ascii \"Fatal error: buffer index out of bounds\\n\"\n");
    out.push_str(".globl _buffer_uaf_msg\n_buffer_uaf_msg:\n    .ascii \"Fatal error: use of buffer after buffer_free()\\n\"\n");
    out.push_str(".globl _buffer_registry_exhausted_msg\n_buffer_registry_exhausted_msg:\n    .ascii \"Fatal error: buffer registry exhausted\\n\"\n");
}

/// Emits the configured heap limit and allocator failure messages.
pub fn append_heap_limits(out: &mut String, heap_size: usize) {
    out.push_str(&format!(".globl _heap_max\n_heap_max:\n    .quad {}\n", heap_size));
    out.push_str(".globl _heap_err_msg\n_heap_err_msg:\n    .ascii \"Fatal error: heap memory exhausted\\n\"\n");
    out.push_str(".globl _heap_dbg_bad_refcount_msg\n_heap_dbg_bad_refcount_msg:\n    .ascii \"Fatal error: heap debug detected bad refcount\\n\"\n");
    out.push_str(".globl _heap_dbg_double_free_msg\n_heap_dbg_double_free_msg:\n    .ascii \"Fatal error: heap debug detected double free\\n\"\n");
    out.push_str(".globl _heap_dbg_free_list_msg\n_heap_dbg_free_list_msg:\n    .ascii \"Fatal error: heap debug detected free-list corruption\\n\"\n");
}

/// Reserves collector state, candidate/proof tables and timing counters.
pub fn append_gc_state(out: &mut String, target: &impl DataTarget) {
    out.push_str(".globl _gc_enabled\n_gc_enabled:\n    .quad 1\n");
    for symbol in [
        "_gc_candidates_enabled", "_gc_candidates_gathering", "_gc_candidates_visited",
        "_gc_candidates_roots", "_gc_candidates_roots_count", "_gc_candidates_roots_capacity",
        "_gc_candidates_nodes", "_gc_candidates_nodes_count", "_gc_candidates_nodes_capacity",
        "_gc_candidates_anchors", "_gc_candidates_anchors_count", "_gc_candidates_anchors_capacity",
        "_gc_candidate_root_scan_fn",
        "_gc_proofs", "_gc_proofs_count", "_gc_proofs_capacity", "_gc_proofs_initialized",
        "_gc_proof_parent", "_gc_proof_slot", "_gc_proofs_debt", "_gc_proofs_visited",
        "_gc_proof_epoch", "_gc_proof_memos", "_gc_proof_memos_capacity", "_gc_proofs_cost",
    ] {
        out.push_str(&target.common(symbol, 8));
    }
    out.push_str(&target.common("_gc_collecting", 8));
    out.push_str(&target.common("_gc_freeing_unreachable", 8));
    out.push_str(&target.common("_gc_pin_head", 8));
    out.push_str(&target.common("_gc_pending_throw", 8));
    out.push_str(&target.common("_gc_release_suppressed", 8));
    out.push_str(&target.common("_hash_write_guard_top", 8));
    out.push_str(&target.common("_gc_runs", 8));
    out.push_str(&target.common("_gc_safe_point_credit", 8));
    out.push_str(&target.common("_gc_safe_point_live_limit", 8));
    out.push_str(&target.common("_gc_collected", 8));
    out.push_str(&target.common("_gc_application_started", 8));
    out.push_str(&target.common("_gc_collector_started", 8));
    out.push_str(&target.common("_gc_free_started", 8));
    out.push_str(&target.common("_gc_destructor_started", 8));
    out.push_str(&target.common("_gc_collector_time", 8));
    out.push_str(&target.common("_gc_destructor_time", 8));
    out.push_str(&target.common("_gc_free_time", 8));
    out.push_str(&target.common("_gc_destructor_depth", 8));
}

/// Reserves generation-safe buffer descriptors and registry cursors.
pub fn append_buffer_registry(out: &mut String, target: &impl DataTarget) {
    out.push_str(&target.common(
        "_buffer_registry",
        (crate::buffers::BUFFER_REGISTRY_CAPACITY + 1)
            * crate::buffers::BUFFER_DESCRIPTOR_SIZE));
    out.push_str(&target.common("_buffer_registry_free", 8));
    out.push_str(".globl _buffer_registry_next\n_buffer_registry_next:\n    .quad 1\n");
}

/// Reserves the aligned heap arena, allocator bins and payload metadata.
pub fn append_heap_storage(out: &mut String, heap_size: usize, target: &impl DataTarget) {
    out.push_str(&target.aligned_common("_heap_buf", heap_size, 16));
    // One bit per 8-byte heap granule records live payload bases. String persistence uses this
    // authoritative side table to reject forged headers in interior byte views in constant time.
    out.push_str(&target.common(
        "_heap_live_bases",
        heap_size.div_ceil(64).max(1)));
    out.push_str(&target.common("_heap_off", 8));
    out.push_str(&target.common("_heap_free_list", 8));
    out.push_str(&target.common(
        "_heap_small_bins",
        crate::heap_consolidate::HEAP_BIN_TABLE_BYTES as usize));
    // One byte per heap granule: set while a guarded free parks the block in a bin.
    out.push_str(&target.common(
        "_heap_bin_marks",
        crate::heap_consolidate::heap_bin_mark_bytes(heap_size)));
    // `_gc_frees` at the end of the allocator's last consolidation pass (see `__rt_heap_alloc`).
    out.push_str(&target.common("_heap_consolidated_frees", 8));
    out.push_str(&target.common("_heap_debug_enabled", 8));
    out.push_str(&target.common("_web_heap_guard_enabled", 8));
}

/// Reserves borrowed-reference and validation-pin root heads.
pub fn append_reference_roots(out: &mut String, target: &impl DataTarget) {
    out.push_str(&target.common("_rt_unmanaged_ref_borrow_top", 8));
    // Active boxed-value validation roots are excluded from PHP-visible sharing counts.
    out.push_str(&target.common("_rt_reference_validation_pins_top", 8));
}

/// Publishes the configured string scratch capacity.
pub fn append_string_scratch_capacity(out: &mut String, heap_size: usize) {
    out.push_str(&format!(".globl _concat_capacity\n_concat_capacity:\n    .quad {}\n",
        crate::strings::concat_scratch::scratch_capacity(heap_size)));
}

/// Reserves the string scratch arena and its allocation cursor.
pub fn append_string_scratch_storage(out: &mut String, heap_size: usize, target: &impl DataTarget) {
    out.push_str(&target.common("_concat_buf", heap_size.max(65_536)));
    out.push_str(&target.common("_concat_off", 8));
}

/// Appends the array allocation-size failure diagnostic.
pub fn append_array_allocation_message(out: &mut String) {
    out.push_str(&format!(
        ".globl _arr_cap_err_msg\n_arr_cap_err_msg:\n    .ascii {:?}\n",
        crate::diagnostics::ARRAY_ALLOC_SIZE_MSG
    ));
}

/// Reserves non-owning handle identities, weak observer slots and the LIFO free stack.
pub fn append_object_handle_storage(out: &mut String, heap_size: usize, target: &impl DataTarget) {
    out.push_str(&target.common("_obj_handle_index", crate::object_handles::object_handle_index_slots(heap_size) * 4));
    out.push_str(&target.common("_obj_weak_reference_index", crate::object_handles::object_handle_index_slots(heap_size) * 8));
    out.push_str(&target.common("_obj_handle_free", crate::object_handles::object_handle_free_slots(heap_size) * 4));
    out.push_str(&target.common("_obj_handle_free_top", 8));
}

/// Starts fresh handle identities at one, reserving zero for blocks without a handle.
pub fn append_object_handle_cursor(out: &mut String) {
    out.push_str(".globl _obj_handle_next\n_obj_handle_next:\n    .quad 1\n");
}
