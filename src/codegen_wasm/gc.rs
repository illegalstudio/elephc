//! Purpose:
//! Emits the hand-authored WAT cycle collector for the wasm32-wasi backend:
//! `__rt_gc_collect_cycles` and its helpers. This is what `Op::GcCollect`
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
//! - The candidate set is deliberately narrow — kinds 2..6 (array, hash, object,
//!   Mixed cell, callable descriptor), and among arrays only those whose elements
//!   are themselves refcounted. A closure's by-value captures are edges. A by-ref
//!   capture (tag 0xFF) owns a kind-8 ref cell, which joins the set only when a live
//!   descriptor names it: the cell does not record what it holds, but the capture
//!   slot's high word does, so `__rt_gc_tag_ref_cells` stamps "this cell holds a heap
//!   pointer" into the cell's transient kind-word bits 17..24 (1 = pointer payload,
//!   255 = descriptors disagree about it) before anything is counted. The exact tag
//!   does not matter: the frame owner and a descriptor name different pointer tags
//!   for one cell (mixed vs callable), and every pointer is validated against its
//!   block header's kind anyway. A cell without a usable stamp stays outside the set
//!   (a leak, never a miscollect).
//! - Marking is iterative: a newly marked block is pushed on an explicit worklist
//!   (one entry per candidate, allocated from the heap for the duration of the
//!   analysis) and drained in a loop, so a very deep graph costs worklist memory
//!   instead of one host stack frame per level.
//! - Destructors run BEFORE anything is freed: the reachability analysis picks the
//!   unreachable objects, each runs its `__destruct` exactly once, and the analysis
//!   is recomputed — a destructor that publishes `$this` (or a partner) makes that
//!   subgraph reachable again, and only what is still unreachable is released. Strings are excluded, which is what makes the reclaim pass safe:
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
    wm.add_global(Global {
        name: "_gc_stack".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    wm.add_global(Global {
        name: "_gc_sp".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    wm.add_raw_func(RT_GC_IS_CANDIDATE);
    wm.add_raw_func(RT_GC_EDGE);
    wm.add_raw_func(RT_GC_MARK_PUSH);
    wm.add_raw_func(RT_GC_MARK_REACHABLE);
    wm.add_raw_func(RT_GC_VISIT_CHILDREN);
    wm.add_raw_func(RT_GC_TAG_REF_CELLS);
    wm.add_raw_func(RT_GC_ANALYZE);
    wm.add_raw_func(RT_GC_CLEAR_TRANSIENT);
    wm.add_raw_func(&rt_gc_destruct_pass(wm.has_tag(super::function::EXCEPTION_TAG)));
    wm.add_raw_func(RT_GC_COLLECT_CYCLES);
}

/// `__rt_gc_is_candidate`: whether the block at header `hdr` participates in cycle
/// collection at all.
///
/// A free-list block (refcount 0) never does. Neither does a string: the collector's
/// reclaim pass is what makes this predicate load-bearing, and a string must stay reachable
/// only through the owner that persisted it. A kind-8 ref cell qualifies only once
/// `__rt_gc_tag_ref_cells` has stamped a pointer payload into its kind word. An indexed array qualifies only when its elements are refcounted (`value_type`
/// 4..7) — a scalar or string array cannot be part of a cycle.
const RT_GC_IS_CANDIDATE: &str = r#"(func $__rt_gc_is_candidate (param $hdr i32) (result i32)
  (local $kindw i64)
  (local $kind i32)
  (local $vt i32)
  (local $stamp i32)
  (if (i32.eqz (i32.load (i32.add (local.get $hdr) (i32.const 4))))
    (then (return (i32.const 0))))                            ;; refcount 0 -> already on the free list
  (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))  ;; kind word @ H+8
  (local.set $kind (i32.and (i32.wrap_i64 (local.get $kindw)) (i32.const 255)))  ;; kind = low byte
  (if (i32.lt_u (local.get $kind) (i32.const 2))
    (then (return (i32.const 0))))                            ;; kinds 0/1 (raw, string) never cycle
  (if (i32.eq (local.get $kind) (i32.const 8))
    (then
      (local.set $stamp
        (i32.and (i32.wrap_i64 (i64.shr_u (local.get $kindw) (i64.const 17))) (i32.const 255)))  ;; what the descriptors stamped
      (return (i32.eq (local.get $stamp) (i32.const 1)))))   ;; a ref cell joins only when every owner agrees it holds a pointer
  (if (i32.gt_u (local.get $kind) (i32.const 6))
    (then (return (i32.const 0))))                            ;; generator frames (7) and anything else are outside the set
  (if (i32.eq (local.get $kind) (i32.const 6))
    (then (return (i32.const 1))))                            ;; callable descriptors own their by-value captures
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
    (then (call $__rt_gc_mark_push (local.get $child)))       ;; mark pass: queue it, never recurse
    (else
      (i64.store (i32.add (local.get $hdr) (i32.const 8))
        (i64.add
          (i64.load (i32.add (local.get $hdr) (i32.const 8)))
          (i64.const 4294967296)))))                          ;; count pass: += 1 in bits 32..63
)
"#;

/// `__rt_gc_mark_push`: sets the reachable bit on `ptr` and queues it on the worklist.
///
/// An already-marked block is left alone, so every candidate is queued at most once and a
/// cycle terminates; that is also what bounds the worklist to one slot per candidate.
const RT_GC_MARK_PUSH: &str = r#"(func $__rt_gc_mark_push (param $ptr i32)
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
  (i32.store (global.get $_gc_sp) (local.get $ptr))           ;; queue the block for its children
  (global.set $_gc_sp (i32.add (global.get $_gc_sp) (i32.const 4)))
)
"#;

/// `__rt_gc_mark_reachable`: marks `ptr` and everything reachable from it.
///
/// Iterative: it queues `ptr`, then pops blocks off the worklist and reports each block's
/// children (which queue themselves) until the worklist is empty. The host stack stays one
/// frame deep however long the chain of references is.
const RT_GC_MARK_REACHABLE: &str = r#"(func $__rt_gc_mark_reachable (param $ptr i32)
  (call $__rt_gc_mark_push (local.get $ptr))                  ;; queue the root itself
  (block $done (loop $drain
    (br_if $done (i32.le_u (global.get $_gc_sp) (global.get $_gc_stack)))  ;; worklist empty
    (global.set $_gc_sp (i32.sub (global.get $_gc_sp) (i32.const 4)))      ;; pop
    (call $__rt_gc_visit_children (i32.load (global.get $_gc_sp)) (i32.const 1))  ;; its children queue themselves
    (br $drain)))
)
"#;

/// `__rt_gc_visit_children`: walks every refcounted child of `ptr` and reports it to
/// `__rt_gc_edge` under `mode`.
///
/// The six layouts, each read exactly as its own runtime reads it:
/// - indexed array: `len` @ +0, `elem_size` @ +16, slots from +24. The stride comes
///   from the stored `elem_size` rather than a constant, which is what keeps an array
///   of Mixed cells (16-byte slots) correct alongside one of objects (8-byte slots).
/// - hash: `capacity` @ +8, 72-byte entries from +40, with the occupied flag at +0,
///   the payload low word at +24 and the runtime value tag at +40 within the entry.
/// - Mixed cell: tag @ +0, low word @ +8.
/// - callable descriptor: `capture_count` @ +12, `capture_tags_ptr` @ +16, 16-byte capture
///   slots from +32 whose low word is the value. The by-value reference tags (4..7, 10, 12)
///   are edges; a string capture is skipped and a by-ref capture (0xFF) is an edge to its
///   ref cell, once `__rt_ref_cell_is_live` has validated the pointer.
/// - ref cell (kind 8): one 32-bit payload pointer @ +0; it is only visited when a
///   descriptor stamped a pointer payload, so the payload is always a heap reference.
/// - object: `class_id` @ +0, 16-byte property slots from +8, with the compile-time
///   property tags read from this class's `gc_desc` descriptor — the same table the
///   object release walk uses. An `AllowDynamicProperties` object's hash tail is
///   visited too, since a dynamic property is an ordinary way to close a cycle.
///
/// Only tags 4..7 (array, hash, object, nested cell) and, in hashes, Mixed cells and
/// captures, tag 10 (callable) are followed. A string child is
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
  (local $cell i32)
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
            (if (i32.or
                  (i32.and
                    (i64.ge_u (local.get $tag) (i64.const 4))
                    (i64.le_u (local.get $tag) (i64.const 7)))
                  (i64.eq (local.get $tag) (i64.const 10)))   ;; array / hash / object / nested cell / callable
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
      (if (i32.or
            (i32.and
              (i64.ge_u (local.get $tag) (i64.const 4))
              (i64.le_u (local.get $tag) (i64.const 7)))
            (i64.eq (local.get $tag) (i64.const 10)))         ;; heap-backed payload (callable included)
        (then
          (call $__rt_gc_edge
            (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))
            (local.get $mode))))                              ;; low word @ C+8
      (return)))

  ;; -- kind 6: callable descriptor with by-value captures --
  (if (i32.eq (local.get $kind) (i32.const 6))
    (then
      (local.set $n (i32.load offset=12 (local.get $ptr)))    ;; capture_count @ D+12
      (local.set $desc (i32.load offset=16 (local.get $ptr))) ;; capture_tags_ptr @ D+16
      (local.set $j (i32.const 0))                            ;; capture cursor
      (block $dend (loop $dwalk
        (br_if $dend (i32.ge_u (local.get $j) (local.get $n)))  ;; visited every capture
        (local.set $ptag (i32.load8_u (i32.add (local.get $desc) (local.get $j))))  ;; capture tag
        (if (i32.eq (local.get $ptag) (i32.const 255))
          (then
            (local.set $cell
              (i32.wrap_i64 (i64.load offset=32 (i32.add (local.get $ptr) (i32.mul (local.get $j) (i32.const 16))))))  ;; ref-cell ptr @ D+32+j*16
            (if (call $__rt_ref_cell_is_live (local.get $cell))
              (then (call $__rt_gc_edge (local.get $cell) (local.get $mode))))))  ;; by-ref capture: edge to the validated cell
        (if (i32.or
              (i32.and (i32.ge_u (local.get $ptag) (i32.const 4)) (i32.le_u (local.get $ptag) (i32.const 7)))
              (i32.or (i32.eq (local.get $ptag) (i32.const 10)) (i32.eq (local.get $ptag) (i32.const 12))))
          (then
            (call $__rt_gc_edge
              (i32.wrap_i64 (i64.load offset=32 (i32.add (local.get $ptr) (i32.mul (local.get $j) (i32.const 16)))))
              (local.get $mode))))                            ;; slot value @ D+32+j*16
        (local.set $j (i32.add (local.get $j) (i32.const 1)))  ;; j++
        (br $dwalk)))
      (return)))

  ;; -- kind 8: ref cell, whose pointer payload the descriptors stamped into the kind word --
  (if (i32.eq (local.get $kind) (i32.const 8))
    (then
      (call $__rt_gc_edge
        (i32.load (local.get $ptr))
        (local.get $mode))                                    ;; payload pointer @ C+0 (candidate => edge-bearing tag)
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

/// `__rt_gc_tag_ref_cells`: stamps each by-ref-captured ref cell as holding a pointer.
///
/// A ref cell is 16 bytes of payload and records nothing about what it holds; the only
/// place that knows is the capture slot of a descriptor (`slot + 8` = payload tag, the same
/// word `__rt_callable_descriptor_release` hands to `__rt_ref_cell_release`). So this walks
/// every live callable descriptor and, for each by-ref capture whose cell passes
/// `__rt_ref_cell_is_live`, writes 1 (tag 4..7, 10 or 12: a heap pointer) or 255 (any other
/// tag) into bits 17..24 of the cell's kind word. A second descriptor that disagrees
/// poisons the stamp (255) so the cell is never traversed under a guess. Which pointer
/// tag it was is irrelevant: the frame owner and a descriptor name different ones for one
/// cell, and `__rt_decref_any` dispatches on the block's own kind. The stamp is transient GC metadata, wiped with the rest
/// by the next pass 1; it must run after pass 1 and before any edge is counted.
const RT_GC_TAG_REF_CELLS: &str = r#"(func $__rt_gc_tag_ref_cells
  (local $hdr i32)
  (local $end i32)
  (local $next i32)
  (local $ptr i32)
  (local $n i32)
  (local $tags i32)
  (local $j i32)
  (local $cell i32)
  (local $chdr i32)
  (local $want i64)
  (local $have i64)
  (local $kindw i64)
  (local.set $end (global.get $__heap_ptr))
  (local.set $hdr (global.get $__heap_base))                  ;; first header
  (block $t_end (loop $t_walk
    (br_if $t_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (local.set $next
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header = H + 16 + size
    (if (i32.and
          (i32.ne (i32.load (i32.add (local.get $hdr) (i32.const 4))) (i32.const 0))  ;; live...
          (i64.eq (i64.and (i64.load (i32.add (local.get $hdr) (i32.const 8))) (i64.const 255)) (i64.const 6)))  ;; ...callable descriptor
      (then
        (local.set $ptr (i32.add (local.get $hdr) (i32.const 16)))
        (local.set $n (i32.load offset=12 (local.get $ptr)))     ;; capture_count @ D+12
        (local.set $tags (i32.load offset=16 (local.get $ptr)))  ;; capture_tags_ptr @ D+16
        (local.set $j (i32.const 0))
        (block $c_end (loop $c_walk
          (br_if $c_end (i32.ge_u (local.get $j) (local.get $n)))  ;; visited every capture
          (if (i32.eq (i32.load8_u (i32.add (local.get $tags) (local.get $j))) (i32.const 255))
            (then
              (local.set $cell
                (i32.wrap_i64 (i64.load offset=32 (i32.add (local.get $ptr) (i32.mul (local.get $j) (i32.const 16))))))
              (if (call $__rt_ref_cell_is_live (local.get $cell))
                (then
                  (local.set $want
                    (i64.and (i64.load offset=40 (i32.add (local.get $ptr) (i32.mul (local.get $j) (i32.const 16)))) (i64.const 255)))  ;; payload tag, from the slot's high word
                  (if (i32.or
                        (i32.or
                          (i32.and (i64.ge_u (local.get $want) (i64.const 4)) (i64.le_u (local.get $want) (i64.const 7)))
                          (i64.eq (local.get $want) (i64.const 10)))
                        (i64.eq (local.get $want) (i64.const 12)))
                    (then (local.set $want (i64.const 1)))      ;; a heap pointer
                    (else (local.set $want (i64.const 255))))   ;; scalar / string / null: never traversable
                  (local.set $chdr (i32.sub (local.get $cell) (i32.const 16)))
                  (local.set $kindw (i64.load (i32.add (local.get $chdr) (i32.const 8))))
                  (local.set $have (i64.and (i64.shr_u (local.get $kindw) (i64.const 17)) (i64.const 255)))
                  (if (i64.eqz (local.get $have))
                    (then (local.set $have (local.get $want)))      ;; first descriptor names the tag
                    (else
                      (if (i64.ne (local.get $have) (local.get $want))
                        (then (local.set $have (i64.const 255))))))  ;; disagreement poisons the stamp
                  (i64.store (i32.add (local.get $chdr) (i32.const 8))
                    (i64.or
                      (i64.and (local.get $kindw) (i64.const -33423361))  ;; clear bits 17..24
                      (i64.shl (local.get $have) (i64.const 17))))))))
          (local.set $j (i32.add (local.get $j) (i32.const 1)))
          (br $c_walk)))))
    (local.set $hdr (local.get $next))                        ;; advance
    (br $t_walk)))
)
"#;

/// `__rt_gc_analyze`: works out which candidates are reachable from outside the graph.
///
/// Three passes over the bump region, the native algorithm unchanged, plus one:
/// 1. clear the transient metadata (bit 16 and the edge count), keeping the low 16 bits
///    the runtime owns; then stamp ref-cell payload tags (`__rt_gc_tag_ref_cells`);
/// 2. count, for every candidate, how many heap edges point AT it (and how many
///    candidates there are, which sizes the worklist);
/// 3. mark reachable from every candidate that is externally referenced: its refcount
///    (flag bits 30/31 masked off) exceeds its incoming edges, or it is mid-destructor
///    (bit 31). That excess is a reference held outside the heap graph, i.e. a live root.
///
/// The worklist is a heap block of one 4-byte slot per candidate, allocated after the
/// count and freed before returning. It is stamped kind 0, so the passes never see it as
/// a candidate. Marked blocks keep bit 16 set on return; unmarked candidates are garbage.
const RT_GC_ANALYZE: &str = r#"(func $__rt_gc_analyze
  (local $hdr i32)
  (local $end i32)
  (local $next i32)
  (local $count i32)
  (local $rc i32)
  (local.set $end (global.get $__heap_ptr))                   ;; one past the last allocated block
  (local.set $count (i32.const 0))

  ;; -- pass 1: clear transient GC metadata, keeping kind / value_type / COW --
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

  ;; -- pass 1b: learn each by-ref-captured ref cell's payload tag from its descriptors --
  (call $__rt_gc_tag_ref_cells)

  ;; -- pass 2: count incoming heap edges and candidates --
  (local.set $hdr (global.get $__heap_base))                  ;; restart at the first header
  (block $n_end (loop $n_walk
    (br_if $n_end (i32.ge_u (local.get $hdr) (local.get $end)))  ;; scanned the whole region
    (local.set $next
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header, captured before any call
    (if (call $__rt_gc_is_candidate (local.get $hdr))
      (then
        (local.set $count (i32.add (local.get $count) (i32.const 1)))
        (call $__rt_gc_visit_children
          (i32.add (local.get $hdr) (i32.const 16))
          (i32.const 0))))                                    ;; mode 0 = count
    (local.set $hdr (local.get $next))                        ;; advance
    (br $n_walk)))

  ;; -- the worklist: one slot per candidate (every block is queued at most once) --
  (global.set $_gc_stack
    (call $__rt_heap_alloc (i32.add (i32.shl (local.get $count) (i32.const 2)) (i32.const 8))))
  (global.set $_gc_sp (global.get $_gc_stack))                ;; empty

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
        (local.set $rc (i32.load (i32.add (local.get $hdr) (i32.const 4))))
        (if (i32.or
              (i32.lt_s (local.get $rc) (i32.const 0))        ;; bit 31: mid-destructor, still in use
              (i64.gt_u
                (i64.extend_i32_u (i32.and (local.get $rc) (i32.const 1073741823)))
                (i64.shr_u (i64.load (i32.add (local.get $hdr) (i32.const 8))) (i64.const 32))))
          (then
            (call $__rt_gc_mark_reachable (i32.add (local.get $hdr) (i32.const 16)))))))  ;; refcount > incoming edges
    (local.set $hdr (local.get $next))                        ;; advance
    (br $m_walk)))

  (call $__rt_heap_free (global.get $_gc_stack))              ;; the worklist has served its purpose
  (global.set $_gc_stack (i32.const 0))
  (global.set $_gc_sp (i32.const 0))
)
"#;

/// `__rt_gc_clear_transient`: strips the collector's transient kind-word state (reachable mark,
/// ref-cell stamps, destructor stamp, incoming-edge count) from every live block, keeping the low
/// 16 bits the runtime owns. Run at the end of a collection and when a destructor aborts one, so
/// the next collection never misreads a leftover mark.
const RT_GC_CLEAR_TRANSIENT: &str = r#"(func $__rt_gc_clear_transient
  (local $hdr i32)
  (local $end i32)
  (local.set $end (global.get $__heap_ptr))
  (local.set $hdr (global.get $__heap_base))                  ;; first header
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
)
"#;

/// `__rt_gc_destruct_pass`: runs the destructor of every object the CURRENT analysis found
/// unreachable and whose destructor has not run; answers how many destructor bodies ran.
///
/// A first walk STAMPS those objects (bit 25 of the kind word: unmarked, kind 4, refcount flag
/// bits 30/31 clear). The second walk calls a destructor only for a stamped object. The stamp is
/// what ties the pass to the analysis it follows: a destructor may free a block ahead of the walk
/// and then allocate, so a brand new object (stored in a global, say, and perfectly live) can land
/// in a hole the walk has yet to reach. It carries no mark, because no analysis saw it, and without
/// the stamp the pass took it for garbage and ran its destructor early. The allocator zeroes the
/// kind word of every block it hands out, which is why a fresh object can never inherit a stamp.
///
/// The other direction is the caller's: an object a destructor CREATED is garbage the next
/// analysis finds, and the caller loops until a pass runs nothing, so it is destructed too, once.
///
/// A destructor that throws abandons the collection (php does the same): the object that threw is
/// flagged as having run, the transient state is cleared, `_gc_collecting` is released and the
/// exception continues. The worklist needs no restoring because `__rt_gc_analyze` has already
/// freed it and reset `_gc_stack`/`_gc_sp` before any destructor runs. Whatever garbage was left
/// is still unreachable and unflagged, so the next collection runs those destructors and frees it.
fn rt_gc_destruct_pass(guard_unwind: bool) -> String {
    let call = if guard_unwind {
        format!(
            r#"        (block $ok
          (block $thrown (result i32)
            (try_table (catch ${tag} $thrown)
              (call $__rt_call_object_destructor (i32.add (local.get $hdr) (i32.const 16))))
            (br $ok))
          ;; the exception pointer is on the stack: leave the collector consistent, then rethrow it
          (i32.store (i32.add (local.get $hdr) (i32.const 4))
            (i32.or
              (i32.and (i32.load (i32.add (local.get $hdr) (i32.const 4))) (i32.const 2147483647))
              (i32.const 1073741824)))                        ;; its destructor has run; drop the in-destructor bit
          (call $__rt_gc_clear_transient)
          (global.set $_gc_collecting (i32.const 0))
          (throw ${tag}))
"#,
            tag = super::function::EXCEPTION_TAG
        )
    } else {
        "        (call $__rt_call_object_destructor (i32.add (local.get $hdr) (i32.const 16)))\n".to_string()
    };
    format!(
        r#"(func $__rt_gc_destruct_pass (result i32)
  (local $hdr i32)
  (local $end i32)
  (local $next i32)
  (local $kindw i64)
  (local $rc i32)
  (local $ran i32)
  (local.set $ran (i32.const 0))
  (local.set $end (global.get $__heap_ptr))

  ;; -- stamp what this analysis says is a garbage object with a destructor still to run --
  (local.set $hdr (global.get $__heap_base))
  (block $s_end (loop $s_walk
    (br_if $s_end (i32.ge_u (local.get $hdr) (local.get $end)))
    (if (call $__rt_gc_is_candidate (local.get $hdr))
      (then
        (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))
        (local.set $rc (i32.load (i32.add (local.get $hdr) (i32.const 4))))
        (if (i32.and
              (i32.and
                (i64.eqz (i64.and (local.get $kindw) (i64.const 65536)))   ;; unreachable...
                (i64.eq (i64.and (local.get $kindw) (i64.const 255)) (i64.const 4)))  ;; ...object...
              (i32.eqz (i32.and (local.get $rc) (i32.const -1073741824))))  ;; ...whose destructor has not run
          (then
            (i64.store (i32.add (local.get $hdr) (i32.const 8))
              (i64.or (local.get $kindw) (i64.const 33554432)))))))  ;; bit 25: destructor due
    (local.set $hdr
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))
    (br $s_walk)))

  ;; -- run the destructors, nothing freed --
  (local.set $hdr (global.get $__heap_base))
  (block $d_end (loop $d_walk
    (br_if $d_end (i32.ge_u (local.get $hdr) (local.get $end)))
    (local.set $next
      (i32.add
        (i32.add (local.get $hdr) (i32.const 16))
        (i32.load (local.get $hdr))))                         ;; next header, captured before the destructor runs
    (if (call $__rt_gc_is_candidate (local.get $hdr))
      (then
        (local.set $kindw (i64.load (i32.add (local.get $hdr) (i32.const 8))))
        (if (i64.ne (i64.and (local.get $kindw) (i64.const 33554432)) (i64.const 0))  ;; stamped by THIS analysis
          (then
            (i64.store (i32.add (local.get $hdr) (i32.const 8))
              (i64.and (local.get $kindw) (i64.const -33554433)))  ;; spend the stamp
{call}            (local.set $rc (i32.load (i32.add (local.get $hdr) (i32.const 4))))
            (if (i32.lt_s (local.get $rc) (i32.const 0))      ;; bit 31 set: a destructor body ran
              (then
                (local.set $ran (i32.add (local.get $ran) (i32.const 1)))
                (i32.store (i32.add (local.get $hdr) (i32.const 4))
                  (i32.or (i32.and (local.get $rc) (i32.const 2147483647)) (i32.const 1073741824)))))))))  ;; clear bit 31, flag "destructor ran"
    (local.set $hdr (local.get $next))
    (br $d_walk)))
  (local.get $ran)
)
"#
    )
}

/// `__rt_gc_collect_cycles`: reclaims every refcounted graph that only its own members
/// still reference. This is what `unset(...)` reaches, and the only reclamation path on
/// this target that a reference cycle cannot defeat.
///
/// 1. `__rt_gc_analyze` marks everything reachable from outside the graph;
/// 2. `__rt_gc_destruct_pass` calls the destructor of every UNMARKED object whose destructor
///    has not run, once, with nothing freed — php runs the destructors of a garbage cycle before
///    releasing any of it. The object is then flagged (bit 30 of its refcount, the same
///    "destructor has run" flag `__rt_decref_object` uses for a resurrected object) so no later
///    release runs it again;
/// 3. when any destructor ran, the analysis runs AGAIN and so does the pass: a destructor may
///    have published `$this` or a partner (a new external reference is a root that keeps the
///    whole subgraph reachable, as in php), and it may have created objects of its own, which
///    are garbage with a destructor still to run. The loop ends when a pass runs nothing, so
///    every object is destructed exactly once and the final analysis is current;
/// 4. every candidate still unmarked is reclaimed.
///
/// The reclaim forces the refcount to one (plus bit 30 for an object, whose destructor
/// has already run) and calls the ordinary `__rt_decref_any`, reusing the exact release
/// path the rest of the runtime is tested against. Freeing a member decrefs its
/// neighbours, which may reclaim them first; the walk re-reads each header, so a block
/// the cascade already freed reads refcount 0 and is skipped instead of freed twice.
///
/// Re-entrancy is suppressed with `_gc_collecting`, because a destructor run during the
/// collection can reach `unset` and would otherwise collect on a half-swept heap. A destructor
/// that throws aborts the collection and releases the flag (see `__rt_gc_destruct_pass`).
const RT_GC_COLLECT_CYCLES: &str = r#"(func $__rt_gc_collect_cycles
  (local $hdr i32)
  (local $end i32)
  (local $next i32)
  (local $kindw i64)
  (if (global.get $_gc_collecting)
    (then (return)))                                          ;; a nested collection would sweep a half-swept heap
  (global.set $_gc_collecting (i32.const 1))                  ;; claim the collector

  ;; -- analyze, run the due destructors, and look again until a pass runs none --
  (block $settled (loop $again
    (call $__rt_gc_analyze)
    (br_if $settled (i32.eqz (call $__rt_gc_destruct_pass)))
    (br $again)))

  ;; -- reclaim every candidate that stayed unmarked --
  (local.set $end (global.get $__heap_ptr))
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
            (i32.store (i32.add (local.get $hdr) (i32.const 4))
              (select (i32.const 1073741825) (i32.const 1)
                (i64.eq (i64.and (local.get $kindw) (i64.const 255)) (i64.const 4))))  ;; force the last reference (objects: destructor done)...
            (if (i64.eq (i64.and (local.get $kindw) (i64.const 255)) (i64.const 8))
              (then
                (call $__rt_ref_cell_release
                  (i32.add (local.get $hdr) (i32.const 16))
                  (i32.const 7)))                             ;; ...a ref cell drops through its own release; tag 7 = a pointer, released by its own kind
              (else
                (call $__rt_decref_any (i32.add (local.get $hdr) (i32.const 16)))))))))  ;; ...anything else through the ordinary release path
    (local.set $hdr (local.get $next))                        ;; advance
    (br $f_walk)))

  ;; -- leave no transient metadata behind for the next collection to misread --
  (call $__rt_gc_clear_transient)
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
    //!   `wasmer`. Runs skip with a SKIPPED line (fail under ELEPHC_TEST_REQUIRE_WASM_HOSTS) when `wasmer` is absent (validation always runs).
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
        emit_mixed_runtime(&mut wm, false, None);
        super::super::float::emit_float_runtime(&mut wm, 0x20000);
        emit_hash_runtime(&mut wm, false);
        emit_object_runtime(&mut wm, false);
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
        if !crate::codegen_wasm::test_support::wasmer_available() {
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

    /// A closure cycle: a boxed callable cell points at a descriptor whose by-value capture
    /// (a Mixed cell) points back at the cell. Callable descriptors used to sit outside the
    /// candidate set, so the edge back into the cell was never counted, the cell looked
    /// externally referenced, and the pair survived every collection. The driver answers
    /// `_gc_live` after dropping the last outside reference and collecting.
    #[test]
    fn collect_cycles_reclaims_a_cycle_through_a_closure_capture() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $c i32)
  (local $d i32)
  (i32.store8 (i32.const 512) (i32.const 7))                      ;; capture tag table: one Mixed capture
  (local.set $c (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $c) (i32.const 8)) (i64.const 5))  ;; kind 5 = Mixed cell
  (local.set $d (call $__rt_heap_alloc (i32.const 48)))
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6))  ;; kind 6 = callable descriptor
  (i64.store (local.get $d) (i64.const 1))                          ;; descriptor kind = Closure
  (i32.store offset=8 (local.get $d) (i32.const 0))
  (i32.store offset=12 (local.get $d) (i32.const 1))                ;; one capture
  (i32.store offset=16 (local.get $d) (i32.const 512))              ;; its tag table
  (i64.store offset=32 (local.get $d) (i64.extend_i32_u (local.get $c)))  ;; capture 0 = the cell (owns a ref)
  (i64.store (local.get $c) (i64.const 10))                         ;; cell tag 10 = callable
  (i64.store offset=8 (local.get $c) (i64.extend_i32_u (local.get $d)))   ;; cell payload = the descriptor
  (call $__rt_incref (local.get $c))                                ;; the descriptor's capture reference
  (call $__rt_decref_any (local.get $c))                            ;; drop the last outside reference
  (call $__rt_gc_collect_cycles)
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0", "the closure cycle was not reclaimed");
        }
    }

    /// A closure that captured itself by reference: a callable descriptor whose by-ref capture
    /// (tag 0xFF) names a kind-8 ref cell, whose payload is the descriptor again. The frame's
    /// own reference to the cell is released, which is the state a returned `$f = function ()
    /// use (&$f) {...}` is left in. The cell used to sit outside the candidate set, so the
    /// edge descriptor -> cell was never counted, the cell looked externally referenced, and
    /// the pair survived every collection. `_gc_live` answers after the collection.
    #[test]
    fn collect_cycles_reclaims_a_cycle_through_a_by_ref_capture() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $cell i32)
  (local $d i32)
  (i32.store8 (i32.const 512) (i32.const 255))                      ;; capture tag table: one by-ref capture
  (local.set $cell (call $__rt_heap_alloc (i32.const 16)))          ;; the frame's reference: refcount 1
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 8))  ;; kind 8 = ref cell
  (local.set $d (call $__rt_heap_alloc (i32.const 48)))
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6))  ;; kind 6 = callable descriptor
  (i64.store (local.get $d) (i64.const 1))                          ;; descriptor kind = Closure
  (i32.store offset=12 (local.get $d) (i32.const 1))                ;; one capture
  (i32.store offset=16 (local.get $d) (i32.const 512))              ;; its tag table
  (call $__rt_ref_cell_incref (local.get $cell))                    ;; the descriptor's owner reference: refcount 2
  (i64.store offset=32 (local.get $d) (i64.extend_i32_u (local.get $cell)))  ;; capture 0 = the cell
  (i64.store offset=40 (local.get $d) (i64.const 10))               ;; its payload tag: callable
  (i64.store (local.get $cell) (i64.extend_i32_u (local.get $d)))   ;; the cell owns the descriptor
  (call $__rt_ref_cell_release (local.get $cell) (i32.const 7))     ;; the frame returns: refcount 1
  (call $__rt_gc_collect_cycles)
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0", "the by-ref closure cycle was not reclaimed");
        }
    }

    /// The control for the by-ref cycle: the SAME graph while the frame still owns the cell
    /// (refcount 2 against one counted edge). That excess is a live root, so the cell, the
    /// descriptor it holds and the capture slot must all survive; a collector that freed
    /// ref cells it merely walked would trap or free the live closure. Answers the kind
    /// bytes of both blocks (8 + 6) and then the live-byte counter's sign.
    #[test]
    fn collect_cycles_spares_a_by_ref_capture_cycle_the_frame_still_owns() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $cell i32)
  (local $d i32)
  (i32.store8 (i32.const 512) (i32.const 255))
  (local.set $cell (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 8))
  (local.set $d (call $__rt_heap_alloc (i32.const 48)))
  (i64.store (i32.sub (local.get $d) (i32.const 8)) (i64.const 6))
  (i64.store (local.get $d) (i64.const 1))
  (i32.store offset=12 (local.get $d) (i32.const 1))
  (i32.store offset=16 (local.get $d) (i32.const 512))
  (call $__rt_ref_cell_incref (local.get $cell))
  (i64.store offset=32 (local.get $d) (i64.extend_i32_u (local.get $cell)))
  (i64.store offset=40 (local.get $d) (i64.const 10))
  (i64.store (local.get $cell) (i64.extend_i32_u (local.get $d)))
  (call $__rt_gc_collect_cycles)                                    ;; the frame still holds its reference
  (i64.add
    (i64.and (i64.load (i32.sub (local.get $cell) (i32.const 8))) (i64.const 255))
    (i64.and (i64.load (i32.sub (local.get $d) (i32.const 8))) (i64.const 255))))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "14", "a by-ref cycle the frame still owns was collected");
        }
    }

    /// A by-ref cycle through a ref cell shared by TWO descriptors: descriptor A captures the
    /// cell by reference and descriptor B by value (a callable capture, tag 10), B captures
    /// the same cell by reference, and the cell holds A. Dropping the frame's reference leaves
    /// a graph the reclaim pass can enter from any of the three blocks; freeing one cascades
    /// into the others, and the walk must neither release the cell twice nor touch a
    /// descriptor the cascade already freed. `_gc_live` must end at zero.
    #[test]
    fn collect_cycles_reclaims_a_ref_cell_shared_by_two_descriptors() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $cell i32)
  (local $a i32)
  (local $b i32)
  (i32.store8 (i32.const 512) (i32.const 255))                      ;; tags A: by-ref cell ...
  (i32.store8 (i32.const 513) (i32.const 10))                       ;; ... and B by value
  (i32.store8 (i32.const 520) (i32.const 255))                      ;; tags B: the same cell by reference
  (local.set $cell (call $__rt_heap_alloc (i32.const 16)))
  (i64.store (i32.sub (local.get $cell) (i32.const 8)) (i64.const 8))
  (local.set $a (call $__rt_heap_alloc (i32.const 64)))
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 6))
  (i64.store (local.get $a) (i64.const 1))
  (i32.store offset=12 (local.get $a) (i32.const 2))
  (i32.store offset=16 (local.get $a) (i32.const 512))
  (local.set $b (call $__rt_heap_alloc (i32.const 48)))
  (i64.store (i32.sub (local.get $b) (i32.const 8)) (i64.const 6))
  (i64.store (local.get $b) (i64.const 1))
  (i32.store offset=12 (local.get $b) (i32.const 1))
  (i32.store offset=16 (local.get $b) (i32.const 520))
  (call $__rt_ref_cell_incref (local.get $cell))                    ;; A's owner reference
  (i64.store offset=32 (local.get $a) (i64.extend_i32_u (local.get $cell)))
  (i64.store offset=40 (local.get $a) (i64.const 10))
  (i64.store offset=48 (local.get $a) (i64.extend_i32_u (local.get $b)))  ;; A owns B (B keeps its allocation reference)
  (call $__rt_ref_cell_incref (local.get $cell))                    ;; B's owner reference
  (i64.store offset=32 (local.get $b) (i64.extend_i32_u (local.get $cell)))
  (i64.store offset=40 (local.get $b) (i64.const 10))
  (i64.store (local.get $cell) (i64.extend_i32_u (local.get $a)))   ;; the cell owns A (A keeps its allocation reference)
  (call $__rt_ref_cell_release (local.get $cell) (i32.const 7))     ;; the frame returns: cell refcount 3 -> 2
  (call $__rt_gc_collect_cycles)
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0", "the shared ref cell cycle was not reclaimed or was freed twice");
        }
    }

    /// A decrement that arrives at a count of zero with only a flag bit set must leave the word
    /// alone. Bit 31 (in a destructor, or mid-release) with count 0 used to become 0x7FFFFFFF and
    /// bit 30 (destructor ran) with count 0 used to become 0xFFFFFFFF: a count of 2^30 or more, so
    /// the object could never be freed again. A one-count object is the control: it is freed.
    #[test]
    fn decref_object_at_a_flags_only_count_is_a_noop() {
        for (flags, expected) in [(2147483648u32, "2147483648"), (1073741824u32, "1073741824")] {
            let driver = format!(
                r#"(func $t (export "t") (result i64)
  (local $o i32)
  (local.set $o (call $__rt_heap_alloc (i32.const 24)))
  (i64.store (i32.sub (local.get $o) (i32.const 8)) (i64.const 4))      ;; kind 4 = object
  (i64.store (local.get $o) (i64.const 0))                              ;; class id 0
  (i32.store (i32.sub (local.get $o) (i32.const 12)) (i32.const {flags}))
  (call $__rt_decref_object (local.get $o))
  (i64.extend_i32_u (i32.load (i32.sub (local.get $o) (i32.const 12)))))"#
            );
            if let Some(o) = run_driver(&driver, "t") {
                assert_eq!(o, expected, "a flags-only refcount {flags:#x} was disturbed");
            }
        }
        let control = r#"(func $t (export "t") (result i64)
  (local $o i32)
  (local.set $o (call $__rt_heap_alloc (i32.const 24)))
  (i64.store (i32.sub (local.get $o) (i32.const 8)) (i64.const 4))
  (i64.store (local.get $o) (i64.const 0))
  (call $__rt_decref_object (local.get $o))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(control, "t") {
            assert_eq!(o, "0", "a one-reference object was not freed");
        }
    }

    /// The worklist is allocated for the analysis and returned afterwards: collecting a
    /// rooted graph twice leaves the live-byte counter exactly where it was.
    #[test]
    fn collect_cycles_returns_its_worklist() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local $before i64)
  (local.set $a (call $__rt_array_new (i64.const 4) (i64.const 8)))
  (i64.store (i32.sub (local.get $a) (i32.const 8)) (i64.const 1026))
  (local.set $before (global.get $_gc_live))
  (call $__rt_gc_collect_cycles)
  (call $__rt_gc_collect_cycles)
  (i64.sub (global.get $_gc_live) (local.get $before)))"#;
        if let Some(o) = run_driver(driver, "t") {
            assert_eq!(o, "0", "the collector leaked its worklist");
        }
    }
}
