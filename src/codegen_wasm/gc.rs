//! Purpose:
//! Emits the hand-authored WAT cycle collector for the wasm32-wasi backend:
//! `__rt_gc_collect_cycles` and its three helpers. This is what `Op::GcCollect`
//! lowers to, and it is the only thing on this target that can reclaim a
//! reference cycle — everything else is refcounting, which a cycle defeats.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for every module (any program may `unset`).
//!
//! Key details:
//! - The algorithm is the native `__rt_gc_collect_cycles`, ported pass for pass:
//!   clear transient metadata, count incoming heap edges, mark everything an
//!   external reference reaches, then reclaim what stayed unmarked. Porting it
//!   rather than inventing one keeps the two backends reclaiming the same graphs.
//! - The heap is walkable because it is a bump region of uniform 16-byte headers
//!   (`H+0` size, `H+4` refcount, `H+8` kind word): stepping `H + 16 + size` from
//!   `__heap_base` to `__heap_ptr` visits every block ever allocated. A block on
//!   the free list has refcount 0, which is how the walk skips it.
//! - Transient GC state lives in the kind word ABOVE the 16 bits the runtime
//!   already uses (low byte kind, bits 8..14 value_type, bit 15 COW): bit 16 is
//!   the reachable mark and bits 32..63 hold the incoming-edge count.
//! - The candidate set is deliberately narrow — kinds 2..5 (array, hash, object,
//!   Mixed cell), and among arrays only those whose elements are themselves
//!   refcounted. Strings are excluded, which is what makes the reclaim pass safe:
//!   a string is only ever freed by the owner that persisted it, so the sweep can
//!   never race an owner's deep-free for one.

use super::wat::{Global, ValType, WatModule};

/// Adds the cycle collector to `wm`. Must be emitted alongside the heap, refcount,
/// array, hash, mixed, and object runtimes, whose globals and release helpers it uses.
pub(super) fn emit_gc_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "_gc_collecting".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    wm.add_raw_func(RT_GC_IS_CANDIDATE);
    wm.add_raw_func(RT_GC_EDGE);
    wm.add_raw_func(RT_GC_MARK_REACHABLE);
    wm.add_raw_func(RT_GC_VISIT_CHILDREN);
    wm.add_raw_func(RT_GC_COLLECT_CYCLES);
}

/// `__rt_gc_is_candidate`: whether the block at header `hdr` participates in cycle
/// collection at all.
///
/// A free-list block (refcount 0) never does. Neither does a string, a ref cell, or a
/// callable descriptor: the collector's reclaim pass is what makes this predicate
/// load-bearing, and a string must stay reachable only through the owner that persisted
/// it. An indexed array qualifies only when its elements are refcounted (`value_type`
/// 4..7) — a scalar or string array cannot be part of a cycle.
const RT_GC_IS_CANDIDATE: &str = r#"(func $__rt_gc_is_candidate (param $hdr i32) (result i32)
  (local $kindw i64)
  (local $kind i32)
  (local $vt i32)
  (if (i32.eqz (i32.load (i32.add (local.get $hdr) (i32.const 4))))
    (then (return (i32.const 0))))                            ;; refcount 0 -> already on the free list
  (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))  ;; kind word @ H+8
  (local.set $kind (i32.and (i32.wrap_i64 (local.get $kindw)) (i32.const 255)))  ;; kind = low byte
  (if (i32.lt_u (local.get $kind) (i32.const 2))
    (then (return (i32.const 0))))                            ;; kinds 0/1 (raw, string) never cycle
  (if (i32.gt_u (local.get $kind) (i32.const 5))
    (then (return (i32.const 0))))                            ;; ref cells (7) and callables (6) are outside the set
  (if (i32.ne (local.get $kind) (i32.const 2))
    (then (return (i32.const 1))))                            ;; hash / object / Mixed cell always qualify
  (local.set $vt (i32.and (i32.wrap_i64 (i64.shr_u (local.get $kindw) (i64.const 8))) (i32.const 127)))  ;; value_type = bits 8..14
  (i32.and
    (i32.ge_u (local.get $vt) (i32.const 4))
    (i32.le_u (local.get $vt) (i32.const 7))))                ;; array of array/hash/object/cell
"#;

/// `__rt_gc_edge`: records one outgoing reference to `child`.
///
/// `mode` 0 counts the edge (used to decide which blocks have references from OUTSIDE
/// the heap graph); `mode` 1 marks the child's whole subgraph reachable. Non-heap,
/// below-heap, and past-the-cursor pointers are ignored, exactly like `__rt_incref`.
const RT_GC_EDGE: &str = r#"(func $__rt_gc_edge (param $child i32) (param $mode i32)
  (local $hdr i32)
  (if (i32.eqz (local.get $child))
    (then (return)))                                          ;; guard: null pointer
  (if (i32.lt_u (local.get $child) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                          ;; guard: below first payload (literal / borrowed)
  (if (i32.ge_u (local.get $child) (global.get $__heap_ptr))
    (then (return)))                                          ;; guard: at/after bump cursor (not live)
  (local.set $hdr (i32.sub (local.get $child) (i32.const 16)))  ;; header of the child block
  (if (i32.eqz (call $__rt_gc_is_candidate (local.get $hdr)))
    (then (return)))                                          ;; outside the collector set -> no edge to track
  (if (local.get $mode)
    (then (call $__rt_gc_mark_reachable (local.get $child)))  ;; mark pass
    (else
      (i64.store (i32.add (local.get $hdr) (i32.const 8))
        (i64.add
          (i64.load (i32.add (local.get $hdr) (i32.const 8)))
          (i64.const 4294967296)))))                          ;; count pass: += 1 in bits 32..63
)
"#;

/// `__rt_gc_mark_reachable`: sets the reachable bit on `ptr` and recurses into its
/// children, stopping at an already-marked block so a cycle terminates.
const RT_GC_MARK_REACHABLE: &str = r#"(func $__rt_gc_mark_reachable (param $ptr i32)
  (local $hdr i32)
  (local $kindw i64)
  (if (i32.eqz (local.get $ptr))
    (then (return)))                                          ;; guard: null pointer
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))
    (then (return)))                                          ;; guard: below first payload
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))
    (then (return)))                                          ;; guard: at/after bump cursor
  (local.set $hdr (i32.sub (local.get $ptr) (i32.const 16)))  ;; header of this block
  (if (i32.eqz (call $__rt_gc_is_candidate (local.get $hdr)))
    (then (return)))                                          ;; outside the collector set
  (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))  ;; kind word @ H+8
  (if (i64.ne (i64.and (local.get $kindw) (i64.const 65536)) (i64.const 0))
    (then (return)))                                          ;; already marked -> the cycle terminates here
  (i64.store (i32.add (local.get $hdr) (i32.const 8))
    (i64.or (local.get $kindw) (i64.const 65536)))            ;; set the reachable bit (bit 16)
  (call $__rt_gc_visit_children (local.get $ptr) (i32.const 1))  ;; recurse through this block's children
)
"#;

/// `__rt_gc_visit_children`: walks every refcounted child of `ptr` and reports it to
/// `__rt_gc_edge` under `mode`.
///
/// The four layouts, each read exactly as its own runtime reads it:
/// - indexed array: `len` @ +0, `elem_size` @ +16, slots from +24. The stride comes
///   from the stored `elem_size` rather than a constant, which is what keeps an array
///   of Mixed cells (16-byte slots) correct alongside one of objects (8-byte slots).
/// - hash: `capacity` @ +8, 72-byte entries from +40, with the occupied flag at +0,
///   the payload low word at +24 and the runtime value tag at +40 within the entry.
/// - Mixed cell: tag @ +0, low word @ +8.
/// - object: `class_id` @ +0, 16-byte property slots from +8, with the compile-time
///   property tags read from this class's `gc_desc` descriptor — the same table the
///   object release walk uses. An `AllowDynamicProperties` object's hash tail is
///   visited too, since a dynamic property is an ordinary way to close a cycle.
///
/// Only tags 4..7 (array, hash, object, nested cell) are followed. A string child is
/// deliberately skipped: strings are outside the candidate set, so counting an edge to
/// one could not change any decision.
const RT_GC_VISIT_CHILDREN: &str = r#"(func $__rt_gc_visit_children (param $ptr i32) (param $mode i32)
  (local $hdr i32)
  (local $kindw i64)
  (local $kind i32)
  (local $vt i32)
  (local $len i64)
  (local $esz i64)
  (local $i i64)
  (local $slot i32)
  (local $cap i64)
  (local $entry i32)
  (local $tag i64)
  (local $n i32)
  (local $cid i32)
  (local $desc i32)
  (local $j i32)
  (local $ptag i32)
  (local $tail i32)
  (local $meta i32)
  (local.set $hdr (i32.sub (local.get $ptr) (i32.const 16)))  ;; header of this block
  (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))  ;; kind word @ H+8
  (local.set $kind (i32.and (i32.wrap_i64 (local.get $kindw)) (i32.const 255)))  ;; kind = low byte

  ;; -- kind 2: indexed array with refcounted elements --
  (if (i32.eq (local.get $kind) (i32.const 2))
    (then
      (local.set $vt (i32.and (i32.wrap_i64 (i64.shr_u (local.get $kindw) (i64.const 8))) (i32.const 127)))  ;; value_type
      (if (i32.and (i32.ge_u (local.get $vt) (i32.const 4)) (i32.le_u (local.get $vt) (i32.const 7)))
        (then
          (local.set $len (i64.load (local.get $ptr)))        ;; length @ A+0
          (local.set $esz (i64.load (i32.add (local.get $ptr) (i32.const 16))))  ;; elem_size @ A+16
          (local.set $i (i64.const 0))                        ;; element cursor
          (block $aend (loop $awalk
            (br_if $aend (i64.ge_s (local.get $i) (local.get $len)))  ;; visited every element
            (local.set $slot
              (i32.add
                (i32.add (local.get $ptr) (i32.const 24))
                (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))  ;; slot = A+24+i*elem_size
            (call $__rt_gc_edge
              (i32.wrap_i64 (i64.load (local.get $slot)))
              (local.get $mode))                              ;; child pointer lives at slot+0
            (local.set $i (i64.add (local.get $i) (i64.const 1)))  ;; i++
            (br $awalk)))))
      (return)))

  ;; -- kind 3: associative hash --
  (if (i32.eq (local.get $kind) (i32.const 3))
    (then
      (local.set $cap (i64.load (i32.add (local.get $ptr) (i32.const 8))))  ;; capacity @ H+8
      (local.set $i (i64.const 0))                            ;; slot cursor
      (block $hend (loop $hwalk
        (br_if $hend (i64.ge_u (local.get $i) (local.get $cap)))  ;; visited every slot
        (local.set $entry
          (i32.add
            (i32.add (local.get $ptr) (i32.const 40))
            (i32.wrap_i64 (i64.mul (local.get $i) (i64.const 72)))))  ;; entry = H+40+i*72
        (if (i64.eq (i64.load (local.get $entry)) (i64.const 1))  ;; occupied == 1 (skip empty and tombstones)
          (then
            (local.set $tag (i64.load (i32.add (local.get $entry) (i32.const 40))))  ;; value tag @ entry+40
            (if (i32.and
                  (i64.ge_u (local.get $tag) (i64.const 4))
                  (i64.le_u (local.get $tag) (i64.const 7)))  ;; array / hash / object / nested cell
              (then
                (call $__rt_gc_edge
                  (i32.wrap_i64 (i64.load (i32.add (local.get $entry) (i32.const 24))))
                  (local.get $mode))))))                      ;; value low word @ entry+24
        (local.set $i (i64.add (local.get $i) (i64.const 1)))  ;; i++
        (br $hwalk)))
      (return)))

  ;; -- kind 5: boxed Mixed cell --
  (if (i32.eq (local.get $kind) (i32.const 5))
    (then
      (local.set $tag (i64.load (local.get $ptr)))            ;; tag @ C+0
      (if (i32.and
            (i64.ge_u (local.get $tag) (i64.const 4))
            (i64.le_u (local.get $tag) (i64.const 7)))        ;; heap-backed payload
        (then
          (call $__rt_gc_edge
            (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))
            (local.get $mode))))                              ;; low word @ C+8
      (return)))

  ;; -- kind 4: object instance --
  (if (i32.ne (local.get $kind) (i32.const 4))
    (then (return)))                                          ;; nothing else has traversable children
  (local.set $cid (i32.wrap_i64 (i64.load (local.get $ptr))))  ;; class_id @ O+0
  ;; The property count comes from the CLASS, not the block: the allocator reuses a free block
  ;; without splitting it, so a size-derived count walks the previous occupant's bytes.
  (local.set $meta (i32.const 0))
  (if (i32.lt_u (local.get $cid) (global.get $__gc_desc_count))
    (then
      (local.set $meta
        (i32.load (i32.add (global.get $__gc_desc_meta) (i32.mul (local.get $cid) (i32.const 4)))))))
  (local.set $n (i32.and (local.get $meta) (i32.const 65535)))  ;; declared property count
  (if (i32.lt_u (local.get $cid) (global.get $__gc_desc_count))
    (then
      (local.set $desc
        (i32.load
          (i32.add (global.get $__gc_desc_ptrs) (i32.mul (local.get $cid) (i32.const 4)))))  ;; desc = ptrs[cid]
      (local.set $j (i32.const 0))                            ;; property cursor
      (block $oend (loop $owalk
        (br_if $oend (i32.ge_u (local.get $j) (local.get $n)))  ;; visited every declared slot
        (local.set $ptag (i32.load8_u (i32.add (local.get $desc) (local.get $j))))  ;; compile-time property tag
        (if (i32.and (i32.ge_u (local.get $ptag) (i32.const 4)) (i32.le_u (local.get $ptag) (i32.const 7)))
          (then
            (call $__rt_gc_edge
              (i32.load offset=8 (i32.add (local.get $ptr) (i32.mul (local.get $j) (i32.const 16))))
              (local.get $mode))))                            ;; slot ptr @ O+8+j*16
        (local.set $j (i32.add (local.get $j) (i32.const 1)))  ;; j++
        (br $owalk)))))
  ;; the dynamic-property hash tail, which the CLASS metadata declares
  (local.set $tail (i32.add (i32.const 8) (i32.mul (local.get $n) (i32.const 16))))  ;; right after the slots
  (if (i32.ne (i32.and (local.get $meta) (i32.const 65536)) (i32.const 0))
    (then
      (call $__rt_gc_edge
        (i32.load (i32.add (local.get $ptr) (local.get $tail)))
        (local.get $mode))))                                  ;; dyn hash @ O + (size-8)
)
"#;

/// `__rt_gc_collect_cycles`: reclaims every refcounted graph that only its own members
/// still reference. This is what `unset(...)` reaches, and the only reclamation path on
/// this target that a reference cycle cannot defeat.
///
/// Four passes over the bump region, the native algorithm unchanged:
/// 1. clear the transient metadata (bit 16 and the edge count), keeping the low 16 bits
///    the runtime owns;
/// 2. count, for every candidate, how many heap edges point AT it;
/// 3. mark reachable from every candidate whose refcount exceeds its incoming edges —
///    that excess is a reference held outside the heap graph, i.e. a live PHP root;
/// 4. reclaim every candidate that stayed unmarked.
///
/// The reclaim forces the refcount to 1 and calls the ordinary `__rt_decref_any` rather
/// than a dedicated free: that reuses the exact release path — destructors included —
/// that the rest of the runtime is tested against. Freeing a member decrefs its
/// neighbours, which may reclaim them first; the walk re-reads each header, so a block
/// the cascade already freed reads refcount 0 and is skipped instead of freed twice.
///
/// Re-entrancy is suppressed with `_gc_collecting`, because a destructor run during the
/// reclaim pass can reach `unset` and would otherwise collect on a half-swept heap.
const RT_GC_COLLECT_CYCLES: &str = r#"(func $__rt_gc_collect_cycles
  (local $hdr i32)
  (local $end i32)
  (local $next i32)
  (local $kindw i64)
  (if (global.get $_gc_collecting)
    (then (return)))                                          ;; a nested collection would sweep a half-swept heap
  (global.set $_gc_collecting (i32.const 1))                  ;; claim the collector

  ;; -- pass 1: clear transient GC metadata, keeping kind / value_type / COW --
  (local.set $end (global.get $__heap_ptr))                   ;; one past the last allocated block
  (local.set $hdr (global.get $__heap_base))                  ;; first header
  (block $c_end (loop $c_walk
    (br_if $c_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (if (i32.ne (i32.load (i32.add (local.get $hdr) (i32.const 4))) (i32.const 0))
      (then
        (i64.store (i32.add (local.get $hdr) (i32.const 8))
          (i64.and
            (i64.load (i32.add (local.get $hdr) (i32.const 8)))
            (i64.const 65535)))))                             ;; keep only the low 16 bits
    (local.set $hdr
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header = H + 16 + size
    (br $c_walk)))

  ;; -- pass 2: count incoming heap edges --
  (local.set $hdr (global.get $__heap_base))                  ;; restart at the first header
  (block $n_end (loop $n_walk
    (br_if $n_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (local.set $next
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header, captured before any call
    (if (call $__rt_gc_is_candidate (local.get $hdr))
      (then
        (call $__rt_gc_visit_children
          (i32.add (local.get $hdr) (i32.const 16))
          (i32.const 0))))                                    ;; mode 0 = count
    (local.set $hdr (local.get $next))                        ;; advance
    (br $n_walk)))

  ;; -- pass 3: mark from every externally-referenced candidate --
  (local.set $hdr (global.get $__heap_base))                  ;; restart at the first header
  (block $m_end (loop $m_walk
    (br_if $m_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (local.set $next
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header, captured before any call
    (if (call $__rt_gc_is_candidate (local.get $hdr))
      (then
        (if (i64.gt_u
              (i64.extend_i32_u (i32.load (i32.add (local.get $hdr) (i32.const 4))))
              (i64.shr_u (i64.load (i32.add (local.get $hdr) (i32.const 8))) (i64.const 32)))
          (then
            (call $__rt_gc_mark_reachable (i32.add (local.get $hdr) (i32.const 16)))))))  ;; refcount > incoming edges
    (local.set $hdr (local.get $next))                        ;; advance
    (br $m_walk)))

  ;; -- pass 4: reclaim every candidate that stayed unmarked --
  (local.set $hdr (global.get $__heap_base))                  ;; restart at the first header
  (block $f_end (loop $f_walk
    (br_if $f_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (local.set $next
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header, captured BEFORE the block is freed
    (if (call $__rt_gc_is_candidate (local.get $hdr))
      (then
        (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))
        (if (i64.eqz (i64.and (local.get $kindw) (i64.const 65536)))
          (then
            (i32.store (i32.add (local.get $hdr) (i32.const 4)) (i32.const 1))  ;; force the last reference...
            (call $__rt_decref_any (i32.add (local.get $hdr) (i32.const 16)))))))  ;; ...and drop it through the ordinary release path
    (local.set $hdr (local.get $next))                        ;; advance
    (br $f_walk)))

  ;; -- leave no transient metadata behind for the next collection to misread --
  (local.set $hdr (global.get $__heap_base))                  ;; restart at the first header
  (block $z_end (loop $z_walk
    (br_if $z_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (if (i32.ne (i32.load (i32.add (local.get $hdr) (i32.const 4))) (i32.const 0))
      (then
        (i64.store (i32.add (local.get $hdr) (i32.const 8))
          (i64.and
            (i64.load (i32.add (local.get $hdr) (i32.const 8)))
            (i64.const 65535)))))                             ;; keep only the low 16 bits
    (local.set $hdr
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header = H + 16 + size
    (br $z_walk)))

  (global.set $_gc_collecting (i32.const 0))                  ;; release the collector
)
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT cycle collector, exercised end-to-end under `wasmer` via a
    //! hand-written driver function and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a reactor module with every runtime the collector walks or
    //!   releases through, validates it with `wasmparser`, and runs the driver under
    //!   `wasmer`. Runs skip silently when `wasmer` is absent (validation always runs).
    //! - The reclaim assertions read `_gc_live`, the allocator's live-byte counter, so a
    //!   block that was merely unmarked rather than freed still fails them.

    use super::emit_gc_runtime;
    use super::super::arrays::emit_array_runtime;
    use super::super::classes::{emit_class_metadata_stub, emit_class_runtime};
    use super::super::closures::emit_closure_runtime;
    use super::super::heap::emit_heap_runtime;
    use super::super::hashes::emit_hash_runtime;
    use super::super::mixed::emit_mixed_runtime;
    use super::super::objects::{
        emit_destructor_dispatch_stub, emit_gc_desc_stub, emit_object_runtime,
    };
    use super::super::refcount::emit_refcount_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_gc_{}_{}", std::process::id(), n))
    }

    /// Builds a 3-page reactor module with every runtime the collector touches plus
    /// `driver`, validates it, and runs `export` under `wasmer`, returning trimmed stdout.
    /// `None` if wasmer is absent; validation always runs.
    fn run_driver(driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_refcount_runtime(&mut wm);
        emit_closure_runtime(&mut wm);
        emit_array_runtime(&mut wm);
        emit_mixed_runtime(&mut wm, false);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        emit_hash_runtime(&mut wm);
        emit_object_runtime(&mut wm);
        emit_gc_desc_stub(&mut wm);
        emit_destructor_dispatch_stub(&mut wm);
        emit_class_metadata_stub(&mut wm);
        emit_class_runtime(&mut wm);
        emit_gc_runtime(&mut wm);
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_err()
        {
            return None;
        }
        let dir = unique_tmp_dir();
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg(export)
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke {export} failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Builds two arrays-of-arrays that reference each other, drops both outside
    /// references, and collects. Refcounting alone leaves both alive forever — each is
    /// held up by the other — so this is exactly the shape `Op::GcCollect` exists for.
    ///
    /// The driver answers `_gc_live` AFTER collecting, so a block the sweep left unmarked
    /// but never freed still fails. The control below runs the identical driver with the
    /// cross-link removed and proves refcounting handles that case on its own, which is
    /// what makes the reclaimed bytes here attributable to the collector.
    #[test]
    fn collect_cycles_reclaims_a_two_block_cycle() {
        // `Op::GcCollect` is the only thing that reaches this runtime, and it lowers to a
        // single call to it — so if the op stops being admitted, the driver below is
        // measuring code no PHP program can run.
        assert!(super::super::capability::op_is_supported(crate::ir::Op::GcCollect));

        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 8)))
  ;; stamp both as arrays whose elements are arrays (kind 2, value_type 4)
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 1026))
  (i64.store (i32.sub (local.get $b) (i32.const 8)) (i64.const 1026))
  ;; a[0] = b, b[0] = a, each holding a counted reference
  (i64.store (i32.add (local.get $a) (i32.const 24)) (i64.extend_i32_u (local.get $b)))
  (i64.store (local.get $a) (i64.const 1))
  (call $__rt_incref (local.get $b))
  (i64.store (i32.add (local.get $b) (i32.const 24)) (i64.extend_i32_u (local.get $a)))
  (i64.store (local.get $b) (i64.const 1))
  (call $__rt_incref (local.get $a))
  ;; drop the two outside references: both blocks are now garbage, both survive
  (call $__rt_decref_any (local.get $a))
  (call $__rt_decref_any (local.get $b))
  (call $__rt_gc_collect_cycles)
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0", "the cycle was not reclaimed");
        }
    }

    /// The control for the test above: the SAME driver without the back-link. Plain
    /// refcounting already frees this, so `_gc_live` reaching 0 here says nothing about
    /// the collector — which is the point. If this ever failed, the reclaim measured
    /// above would not be attributable to cycle collection.
    #[test]
    fn refcounting_alone_reclaims_an_acyclic_chain() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 1026))
  (i64.store (i32.sub (local.get $b) (i32.const 8)) (i64.const 1026))
  (i64.store (i32.add (local.get $a) (i32.const 24)) (i64.extend_i32_u (local.get $b)))
  (i64.store (local.get $a) (i64.const 1))
  (call $__rt_incref (local.get $b))
  (call $__rt_decref_any (local.get $a))
  (call $__rt_decref_any (local.get $b))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0", "an acyclic chain should not need the collector");
        }
    }

    /// A live root must SURVIVE a collection. `$a` still holds a counted reference, so
    /// its refcount exceeds its incoming edges and the mark pass reaches its whole graph.
    /// Without that rule the collector would free everything it walks, which the output
    /// of an ordinary program would not always reveal.
    #[test]
    fn collect_cycles_spares_a_rooted_cycle() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $b i32)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (local.set $b (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 1026))
  (i64.store (i32.sub (local.get $b) (i32.const 8)) (i64.const 1026))
  (i64.store (i32.add (local.get $a) (i32.const 24)) (i64.extend_i32_u (local.get $b)))
  (i64.store (local.get $a) (i64.const 1))
  (call $__rt_incref (local.get $b))
  (i64.store (i32.add (local.get $b) (i32.const 24)) (i64.extend_i32_u (local.get $a)))
  (i64.store (local.get $b) (i64.const 1))
  (call $__rt_incref (local.get $a))
  ;; drop only b's outside reference: a is still rooted, so the whole cycle is live
  (call $__rt_decref_any (local.get $b))
  (call $__rt_gc_collect_cycles)
  ;; both blocks must still carry their kind byte, which a freed block would not
  (i64.add
    (i64.and (i64.load (i32.sub (local.get $a) (i32.const 8))) (i64.const 255))
    (i64.and (i64.load (i32.sub (local.get $b) (i32.const 8))) (i64.const 255))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "4", "a rooted cycle was collected");
        }
    }
}
