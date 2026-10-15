//! Purpose:
//! Object identity on `wasm32-wasi`: the handle pool `spl_object_id()` reads.
//!
//! Called from:
//! - `plan` registers the runtime for a module that asks an object for its identity.
//! - `objects::emit_object_allocation` registers every new object, and `__rt_decref_object`
//!   forgets it again on the way to `__rt_heap_free`.
//! - `builtins` lowers `RuntimeFnId::SplObjectId` to `__rt_object_handle_of`.
//!
//! Key details:
//! - The handle is NOT stored in the object. The object payload starts with the class id at
//!   `+0` and a 16-byte slot per declared property after it, and every class-id read in this
//!   backend — instanceof, method dispatch, the release walk, the cycle collector — loads that
//!   word as a whole `i64`. Widening the header or splitting that word would mean changing all
//!   of them in step, and a missed one is a silently wrong class rather than a failure. A side
//!   table touches nothing.
//! - Finding an object's slot by its address is a hash lookup, not a scan: a reverse index of
//!   open-addressing cells (pointer to slot index plus one, with tombstones) rides beside the
//!   slot block, so `spl_object_id()` and the release in `__rt_decref_object` stay constant-time
//!   however many objects are alive. The index only locates slots, so the ids themselves are
//!   exactly the ones the scan answered.
//! - Handles are assigned at ALLOCATION, not on first query. php's are, and the difference is
//!   observable: `$a = new A(); $b = new A(); echo spl_object_id($b);` answers 2 in php, and
//!   would answer 1 under lazy assignment because `$b` asked first.
//! - A freed handle is reused LIFO, which is what php's object store does with its free-list
//!   head. `$a = new A(); unset($a); $b = new A();` gives `$b` the handle `$a` had.
//! - The pool only exists in a module that asks. Nothing else in the backend needs object
//!   identity, so registration is not on the allocation path of a program that never calls
//!   `spl_object_id()`, and `__rt_decref_object` does not carry the call.

use crate::ir::{Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId};

use super::wat::{Global, ValType, WatModule};

/// Slots in the pool's first allocation.
///
/// One block holds both halves — the live pointers and the free stack — so this is 128 bytes.
/// A program that asks an object for its identity usually has few of them.
const POOL_START_SLOTS: u32 = 16;

/// Largest pool this runtime will grow to, in slots.
///
/// A million live objects is past what this target serves; beyond it registration fails and the
/// object has no handle, which `spl_object_id()` answers as `0` rather than trapping.
const POOL_MAX_SLOTS: u32 = 1 << 20;

/// Whether any instruction in the module asks an object for its identity.
///
/// The gate for the whole pool: the globals, the runtime, the stamp in `emit_object_allocation`
/// and the release call in `__rt_decref_object` all key off this one answer, so they can never
/// disagree about whether the table exists.
pub(super) fn module_uses_object_handles(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(|function| {
            function.instructions.iter().any(|inst| {
                inst.op == Op::RuntimeCall
                    && matches!(
                        inst.immediate.as_ref(),
                        Some(Immediate::RuntimeCall(
                            RuntimeCallTarget::Function(RuntimeFnId::SplObjectId)
                                | RuntimeCallTarget::ProfiledFunction {
                                    target: RuntimeFnId::SplObjectId,
                                    ..
                                }
                        ))
                    )
            })
        })
}

/// Adds the object-handle pool to `wm`.
///
/// Requires the heap runtime (`__rt_heap_alloc` / `__rt_heap_free`). Registered only for a
/// module `module_uses_object_handles` answered true for.
pub(super) fn emit_handle_runtime(wm: &mut WatModule) {
    for name in [
        "__obj_slots",
        "__obj_slot_cap",
        "__obj_slot_used",
        "__obj_free_top",
        "__obj_index",
        "__obj_index_fill",
    ] {
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: true,
            init: 0,
        });
    }
    wm.add_raw_func(&rt_handle_reserve());
    wm.add_raw_func(RT_HANDLE_INDEX);
    wm.add_raw_func(RT_HANDLE_REGISTER);
    wm.add_raw_func(RT_HANDLE_FORGET);
    wm.add_raw_func(RT_OBJECT_HANDLE_OF);
}

/// `__rt_handle_reserve`: makes room for one more never-used slot. 1 on success, 0 on refusal.
///
/// One block carries both halves: `cap` live pointers at `+0`, then `cap` free-stack entries.
/// The free stack can never be deeper than the number of slots handed out, so sizing it with
/// the pool is exact rather than generous. Growing MOVES the free stack, since its base is
/// derived from `cap` — which is safe only because the stack is empty whenever the pool grows.
///
/// The new block is zeroed before the copy. `__rt_heap_alloc` reuses a freed block WITHOUT
/// splitting it, so what it returns is whatever the previous occupant left there — and a
/// non-zero word in the live half would be read as an object pointer by the scans below.
fn rt_handle_reserve() -> String {
    format!(
        r#"(func $__rt_handle_reserve (result i32)
  (local $cap i32) (local $new i32) (local $blk i32) (local $old i32) (local $idx_blk i32)
  (if (i32.lt_u (global.get $__obj_slot_used) (global.get $__obj_slot_cap))
    (then (return (i32.const 1))))                                  ;; a never-used slot is already there
  (local.set $cap (global.get $__obj_slot_cap))
  (local.set $new (select (i32.shl (local.get $cap) (i32.const 1))
                          (i32.const {start})
                          (local.get $cap)))                        ;; double, or the first size
  (if (i32.gt_u (local.get $new) (i32.const {max})) (then (return (i32.const 0))))
  (local.set $blk (call $__rt_heap_alloc (i32.shl (local.get $new) (i32.const 3))))
  (if (i32.eqz (local.get $blk)) (then (return (i32.const 0))))
  (memory.fill (local.get $blk) (i32.const 0) (i32.shl (local.get $new) (i32.const 3)))
  ;; the reverse index holds twice as many cells as there are slots, so it is never past half full
  ;; of live entries; it is rebuilt from the slot block below
  (local.set $idx_blk (call $__rt_heap_alloc (i32.shl (local.get $new) (i32.const 3))))
  (if (i32.eqz (local.get $idx_blk))
    (then (call $__rt_heap_free (local.get $blk)) (return (i32.const 0))))
  (local.set $old (global.get $__obj_slots))
  (if (local.get $old) (then
    (memory.copy (local.get $blk) (local.get $old)
                 (i32.shl (local.get $cap) (i32.const 2)))           ;; the live pointers
    ;; The free stack is NOT copied, because it is empty here: the only caller checks it first
    ;; and grows only when there was nothing to reuse. Copying it anyway would be a branch
    ;; nothing can reach, and so a branch nothing can test.
    (call $__rt_heap_free (local.get $old))))
  (global.set $__obj_slots (local.get $blk))
  (global.set $__obj_slot_cap (local.get $new))
  (if (global.get $__obj_index) (then (call $__rt_heap_free (global.get $__obj_index))))
  (global.set $__obj_index (local.get $idx_blk))
  (call $__rt_handle_index_rebuild)                                  ;; the new cap changes the mask
  (i32.const 1))
"#,
        start = POOL_START_SLOTS,
        max = POOL_MAX_SLOTS
    )
}

/// The reverse index from an object's address to its slot.
///
/// `__obj_index` holds `2 * cap` i32 cells. A cell is 0 when empty, `-1` when it is a tombstone
/// left by a forgotten object, and otherwise a slot index plus one. Probing is linear from a
/// multiplicative hash of the address, and a lookup stops at the first EMPTY cell, so the table
/// must always keep some: `__rt_handle_index_insert` rebuilds it (dropping every tombstone)
/// once the non-empty cells pass three quarters, which the churn of freeing and allocating
/// objects at different addresses would otherwise reach.
///
/// - `__rt_handle_hash`: the first cell to try for an address.
/// - `__rt_handle_index_put`: places a slot in the first empty or tombstone cell.
/// - `__rt_handle_index_rebuild`: clears the table and re-inserts every live slot.
/// - `__rt_handle_index_insert`: `put` after making room; called BEFORE the slot is written, so a
///   rebuild never sees the object twice.
/// - `__rt_handle_probe`: `(cell, slot)` for an address, with slot `-1` when it is not registered.
const RT_HANDLE_INDEX: &str = r#"(func $__rt_handle_hash (param $obj i32) (result i32)
  (local $h i32)
  (local.set $h (i32.mul (i32.shr_u (local.get $obj) (i32.const 3)) (i32.const -1640531535)))
  (i32.and (i32.xor (local.get $h) (i32.shr_u (local.get $h) (i32.const 15)))
           (i32.sub (i32.shl (global.get $__obj_slot_cap) (i32.const 1)) (i32.const 1))))

(func $__rt_handle_index_put (param $obj i32) (param $slot i32)
  (local $mask i32) (local $at i32) (local $e i32)
  (local.set $mask (i32.sub (i32.shl (global.get $__obj_slot_cap) (i32.const 1)) (i32.const 1)))
  (local.set $at (call $__rt_handle_hash (local.get $obj)))
  (block $placed (loop $probe
    (local.set $e (i32.load (i32.add (global.get $__obj_index) (i32.shl (local.get $at) (i32.const 2)))))
    (br_if $placed (i32.or (i32.eqz (local.get $e)) (i32.eq (local.get $e) (i32.const -1))))
    (local.set $at (i32.and (i32.add (local.get $at) (i32.const 1)) (local.get $mask)))
    (br $probe)))
  (if (i32.eqz (local.get $e))                                       ;; a fresh cell fills; a tombstone is reused
    (then (global.set $__obj_index_fill (i32.add (global.get $__obj_index_fill) (i32.const 1)))))
  (i32.store (i32.add (global.get $__obj_index) (i32.shl (local.get $at) (i32.const 2)))
             (i32.add (local.get $slot) (i32.const 1))))

(func $__rt_handle_index_rebuild
  (local $i i32) (local $p i32)
  (memory.fill (global.get $__obj_index) (i32.const 0)
               (i32.shl (global.get $__obj_slot_cap) (i32.const 3)))
  (global.set $__obj_index_fill (i32.const 0))
  (block $done (loop $each
    (br_if $done (i32.ge_u (local.get $i) (global.get $__obj_slot_used)))
    (local.set $p (i32.load (i32.add (global.get $__obj_slots) (i32.shl (local.get $i) (i32.const 2)))))
    (if (local.get $p) (then (call $__rt_handle_index_put (local.get $p) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $each))))

(func $__rt_handle_index_insert (param $obj i32) (param $slot i32)
  (if (i32.gt_u (i32.mul (i32.add (global.get $__obj_index_fill) (i32.const 1)) (i32.const 4))
                (i32.mul (i32.shl (global.get $__obj_slot_cap) (i32.const 1)) (i32.const 3)))
    (then (call $__rt_handle_index_rebuild)))
  (call $__rt_handle_index_put (local.get $obj) (local.get $slot)))

(func $__rt_handle_probe (param $obj i32) (result i32 i32)
  (local $mask i32) (local $at i32) (local $e i32)
  (if (i32.eqz (global.get $__obj_index)) (then (return (i32.const 0) (i32.const -1))))
  (local.set $mask (i32.sub (i32.shl (global.get $__obj_slot_cap) (i32.const 1)) (i32.const 1)))
  (local.set $at (call $__rt_handle_hash (local.get $obj)))
  (loop $probe
    (local.set $e (i32.load (i32.add (global.get $__obj_index) (i32.shl (local.get $at) (i32.const 2)))))
    (if (i32.eqz (local.get $e)) (then (return (local.get $at) (i32.const -1))))   ;; never registered
    (if (i32.ne (local.get $e) (i32.const -1))
      (then
        (if (i32.eq (i32.load (i32.add (global.get $__obj_slots)
                      (i32.shl (i32.sub (local.get $e) (i32.const 1)) (i32.const 2))))
                    (local.get $obj))
          (then (return (local.get $at) (i32.sub (local.get $e) (i32.const 1)))))))
    (local.set $at (i32.and (i32.add (local.get $at) (i32.const 1)) (local.get $mask)))
    (br $probe))
  (i32.const 0) (i32.const -1))                                    ;; the loop only leaves by return
"#;

/// `__rt_handle_register`: gives `$obj` the handle it keeps for its whole life. 0 when refused.
///
/// A freed slot is taken before a fresh one, most recently freed first, which is the order
/// php's object store reuses its own: its free-list head is pushed on free and popped on
/// allocate. The handle is the slot index plus one, so handles start at 1 like php's.
const RT_HANDLE_REGISTER: &str = r#"(func $__rt_handle_register (param $obj i32) (result i32)
  (local $idx i32)
  (if (global.get $__obj_free_top) (then                             ;; a slot was freed: reuse it
    (global.set $__obj_free_top (i32.sub (global.get $__obj_free_top) (i32.const 1)))
    (local.set $idx (i32.load (i32.add
      (i32.add (global.get $__obj_slots) (i32.shl (global.get $__obj_slot_cap) (i32.const 2)))
      (i32.shl (global.get $__obj_free_top) (i32.const 2)))))
    (call $__rt_handle_index_insert (local.get $obj) (local.get $idx))
    (i32.store (i32.add (global.get $__obj_slots) (i32.shl (local.get $idx) (i32.const 2)))
               (local.get $obj))
    (return (i32.add (local.get $idx) (i32.const 1)))))
  (if (i32.eqz (call $__rt_handle_reserve)) (then (return (i32.const 0))))
  (local.set $idx (global.get $__obj_slot_used))
  (global.set $__obj_slot_used (i32.add (local.get $idx) (i32.const 1)))
  (call $__rt_handle_index_insert (local.get $obj) (local.get $idx))
  (i32.store (i32.add (global.get $__obj_slots) (i32.shl (local.get $idx) (i32.const 2)))
             (local.get $obj))
  (i32.add (local.get $idx) (i32.const 1)))
"#;

/// `__rt_handle_forget`: releases `$obj`'s slot back to the free stack.
///
/// Called from `__rt_decref_object` immediately before the storage is freed. Forgetting is not
/// optional bookkeeping: `__rt_heap_alloc` hands the same address back to the next object, so a
/// stale row would make two different objects share one handle — and the scan, which stops at
/// the first match, would answer the dead one's.
const RT_HANDLE_FORGET: &str = r#"(func $__rt_handle_forget (param $obj i32)
  (local $at i32) (local $slot i32)
  (if (i32.eqz (global.get $__obj_slots)) (then (return)))
  (call $__rt_handle_probe (local.get $obj))
  (local.set $slot)
  (local.set $at)
  (if (i32.lt_s (local.get $slot) (i32.const 0)) (then (return)))      ;; never registered
  (i32.store (i32.add (global.get $__obj_index) (i32.shl (local.get $at) (i32.const 2)))
             (i32.const -1))                                         ;; tombstone: probes pass through it
  (i32.store (i32.add (global.get $__obj_slots) (i32.shl (local.get $slot) (i32.const 2)))
             (i32.const 0))                                          ;; the slot is free again
  (i32.store (i32.add
    (i32.add (global.get $__obj_slots) (i32.shl (global.get $__obj_slot_cap) (i32.const 2)))
    (i32.shl (global.get $__obj_free_top) (i32.const 2)))
    (local.get $slot))                                               ;; push it for the next object
  (global.set $__obj_free_top (i32.add (global.get $__obj_free_top) (i32.const 1))))
"#;

/// `__rt_object_handle_of`: the handle `$obj` was given at allocation, or 0.
///
/// Zero is not a handle php can answer, so it reads as "this object was never registered" —
/// which only happens when the pool refused to grow.
const RT_OBJECT_HANDLE_OF: &str = r#"(func $__rt_object_handle_of (param $obj i32) (result i64)
  (local $at i32) (local $slot i32)
  (if (i32.eqz (local.get $obj)) (then (return (i64.const 0))))
  (if (i32.eqz (global.get $__obj_slots)) (then (return (i64.const 0))))
  (call $__rt_handle_probe (local.get $obj))
  (local.set $slot)
  (local.set $at)
  (if (i32.lt_s (local.get $slot) (i32.const 0)) (then (return (i64.const 0))))
  (i64.extend_i32_u (i32.add (local.get $slot) (i32.const 1))))
"#;
