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
/// Sixty-seven million slots (a 512 MB table and as much again for its index) is past what this
/// target serves; beyond it registration raises the
/// named "Allowed memory size exhausted" fatal. An object without a slot would be invisible to the
/// shutdown walk and silently skip its destructor, so refusing loudly is the only safe answer.
const POOL_MAX_SLOTS: u32 = 1 << 26;

/// Whether any instruction in the module asks an object for its identity.
///
/// The gate for the whole pool: the globals, the runtime, the stamp in `emit_object_allocation`
/// and the release call in `__rt_decref_object` all key off this one answer, so they can never
/// disagree about whether the table exists.
pub(super) fn module_uses_object_handles(module: &Module) -> bool {
    module_has_destructors(module)
        || module
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

/// Whether any class in the module declares a `__destruct`.
///
/// Such a module needs the pool even if nothing asks for an object's identity: php ends a script
/// by running the destructors of the objects still alive in the order their handles were handed
/// out, and the pool is the only record of which objects those are and in what order.
pub(super) fn module_has_destructors(module: &Module) -> bool {
    let key = crate::names::php_symbol_key("__destruct");
    module
        .class_infos
        .values()
        .any(|info| info.methods.contains_key(&key))
}

/// Name of the flag that switches destructors off once script end has run them.
pub(super) const DTOR_OFF_GLOBAL: &str = "__dtor_off";

/// Emits `__rt_run_all_destructors`: php's `zend_objects_store_call_destructors`.
///
/// Walks the handle pool from the first slot up, running each live object's `__destruct` once
/// and marking it with the same "destructor ran" flag (bit 30) the cycle collector uses, so the
/// release that frees it later does not run it again. The bound is re-read every step because a
/// destructor can create objects, and the base pointer too because creating one can grow the
/// pool and move it. php's walk behaves the same way, so an object that takes a slot already
/// passed is not visited. Generator frames share the pool and are skipped: only a header kind
/// of 4 (object) has a destructor. When the walk ends it sets the off flag, after which
/// `__rt_call_object_destructor` is a no-op, because php runs no destructor past this point.
pub(super) fn emit_shutdown_walk(wm: &mut WatModule) {
    wm.add_raw_func(
        r#"(func $__rt_run_all_destructors
  (local $i i32) (local $p i32) (local $rc i32)
  (global.set $__obj_no_reuse (i32.const 1))                                                ;; php stops reusing handles here, so an object a destructor creates takes a NEW slot this walk reaches
  (block $done
    (loop $next
      (br_if $done (i32.ge_u (local.get $i) (global.get $__obj_slot_used)))
      (local.set $p (i32.load (i32.add (global.get $__obj_slots) (i32.shl (local.get $i) (i32.const 2)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (if (local.get $p) (then
        (if (i32.eq (i32.load8_u (i32.sub (local.get $p) (i32.const 8))) (i32.const 4)) (then   ;; an object, not a generator frame
          (local.set $rc (i32.load (i32.sub (local.get $p) (i32.const 12))))
          (if (i32.eqz (i32.and (local.get $rc) (i32.const -1073741824))) (then              ;; alive, destructor not yet run
            (call $__rt_call_object_destructor (local.get $p))
            (local.set $rc (i32.load (i32.sub (local.get $p) (i32.const 12))))
            (if (i32.lt_s (local.get $rc) (i32.const 0)) (then                                ;; a destructor body ran
              ;; An object whose destructor dropped the last reference to itself stays allocated, and
              ;; so does php's: the walk only lends it a reference for the call and the object store
              ;; frees it later without a destructor. Freeing it here would run its children's
              ;; destructors EARLY (measured: php runs them when the walk reaches them).
              (i32.store (i32.sub (local.get $p) (i32.const 12))
                (i32.or (i32.and (local.get $rc) (i32.const 2147483647)) (i32.const 1073741824)))))))))))
      (br $next)))
  (global.set $__dtor_off (i32.const 1)))
"#,
    );
}

/// Adds the object-handle pool to `wm`.
///
/// Requires the heap runtime (`__rt_heap_alloc` / `__rt_heap_free`). Registered only for a
/// module `module_uses_object_handles` answered true for.
pub(super) fn emit_handle_runtime(wm: &mut WatModule) {
    emit_handle_runtime_with_max(wm, POOL_MAX_SLOTS);
}

/// `emit_handle_runtime` with the pool ceiling chosen by the caller, so a unit test can reach it.
fn emit_handle_runtime_with_max(wm: &mut WatModule, max_slots: u32) {
    for name in [
        "__obj_slots",
        "__obj_slot_cap",
        "__obj_slot_used",
        "__obj_free_top",
        "__obj_index",
        "__obj_index_fill",
        // Set while the shutdown walk runs: freed handles are no longer handed out again.
        "__obj_no_reuse",
    ] {
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: true,
            init: 0,
        });
    }
    wm.add_raw_func(&rt_handle_reserve(max_slots));
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
/// derived from `cap`, so the entries are copied along (the stack is non-empty at a grow only
/// while the shutdown walk has handle reuse off).
///
/// The new block is zeroed before the copy. `__rt_heap_alloc` reuses a freed block WITHOUT
/// splitting it, so what it returns is whatever the previous occupant left there — and a
/// non-zero word in the live half would be read as an object pointer by the scans below.
fn rt_handle_reserve(max_slots: u32) -> String {
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
    ;; The free stack moves with the pool. It is usually empty here (registration reuses a freed
    ;; slot before growing), but not while the shutdown walk runs, when reuse is off.
    (memory.copy (i32.add (local.get $blk) (i32.shl (local.get $new) (i32.const 2)))
                 (i32.add (local.get $old) (i32.shl (local.get $cap) (i32.const 2)))
                 (i32.shl (global.get $__obj_free_top) (i32.const 2)))
    (call $__rt_heap_free (local.get $old))))
  (global.set $__obj_slots (local.get $blk))
  (global.set $__obj_slot_cap (local.get $new))
  (if (global.get $__obj_index) (then (call $__rt_heap_free (global.get $__obj_index))))
  (global.set $__obj_index (local.get $idx_blk))
  (call $__rt_handle_index_rebuild)                                  ;; the new cap changes the mask
  (i32.const 1))
"#,
        start = POOL_START_SLOTS,
        max = max_slots
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

/// `__rt_handle_register`: gives `$obj` the handle it keeps for its whole life. A full pool is a
/// fatal (see `POOL_MAX_SLOTS`), never a silent 0.
///
/// A freed slot is taken before a fresh one, most recently freed first, which is the order
/// php's object store reuses its own: its free-list head is pushed on free and popped on
/// allocate. The handle is the slot index plus one, so handles start at 1 like php's.
const RT_HANDLE_REGISTER: &str = r#"(func $__rt_handle_register (param $obj i32) (result i32)
  (local $idx i32)
  (if (i32.and (i32.ne (global.get $__obj_free_top) (i32.const 0)) (i32.eqz (global.get $__obj_no_reuse))) (then   ;; a slot was freed: reuse it (php stops once the shutdown walk starts)
    (global.set $__obj_free_top (i32.sub (global.get $__obj_free_top) (i32.const 1)))
    (local.set $idx (i32.load (i32.add
      (i32.add (global.get $__obj_slots) (i32.shl (global.get $__obj_slot_cap) (i32.const 2)))
      (i32.shl (global.get $__obj_free_top) (i32.const 2)))))
    (call $__rt_handle_index_insert (local.get $obj) (local.get $idx))
    (i32.store (i32.add (global.get $__obj_slots) (i32.shl (local.get $idx) (i32.const 2)))
               (local.get $obj))
    (return (i32.add (local.get $idx) (i32.const 1)))))
  (if (i32.eqz (call $__rt_handle_reserve))
    (then (call $__rt_oom) unreachable))                           ;; elephc-trap:deterministic-oom:handle-pool-full an object with no slot would silently skip its shutdown destructor
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

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the handle pool's growth and reuse rules, run under `wasmer` with a
    //! hand-written driver, the way the heap and collector tests are.
    //!
    //! Key details:
    //! - Objects are bare pointers: registration only hashes the address, so nothing is
    //!   dereferenced and no real object is needed.
    //! - Runs skip when `wasmer` is absent (and fail under ELEPHC_TEST_REQUIRE_WASM_HOSTS).

    use super::super::heap::emit_heap_runtime;
    use super::{emit_handle_runtime_with_max, WatModule};
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Builds a reactor with the heap and a handle pool capped at `max_slots`, validates it, runs
    /// `export` under `wasmer`, and returns (succeeded, trimmed stdout). `None` without wasmer.
    fn run_driver(max_slots: u32, driver: &str, export: &str) -> Option<(bool, String)> {
        let mut wm = WatModule::new();
        wm.set_memory(3, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, 3 * 65536);
        emit_handle_runtime_with_max(&mut wm, max_slots);
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !crate::codegen_wasm::test_support::wasmer_available() {
            return None;
        }
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("elephc_wasm_handles_{}_{}", std::process::id(), n));
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
        Some((out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }

    /// The loop registering `count` distinct fake objects starting at index `from`.
    fn register_loop(from: u32, count: u32) -> String {
        format!(
            r#"  (local.set $i (i32.const {from}))
  (block $d{from} (loop $l{from}
    (br_if $d{from} (i32.ge_u (local.get $i) (i32.const {end})))
    (drop (call $__rt_handle_register (i32.add (i32.const 4096) (i32.mul (local.get $i) (i32.const 16)))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $l{from})))
"#,
            end = from + count
        )
    }

    /// A pool at its ceiling answers with a trap rather than a silent 0: an object without a
    /// slot would never be visited by the shutdown walk. The control fills the pool exactly.
    #[test]
    fn a_full_pool_is_a_fatal_not_a_silent_refusal() {
        let control = format!(
            "(func $t (export \"t\") (result i64) (local $i i32)\n{}  (i64.extend_i32_u (global.get $__obj_slot_used)))",
            register_loop(0, 16)
        );
        if let Some((ok, out)) = run_driver(16, &control, "t") {
            assert!(ok, "a pool filled to its ceiling must still work");
            assert_eq!(out, "16");
        }
        let over = format!(
            "(func $t (export \"t\") (result i64) (local $i i32)\n{}  (i64.extend_i32_u (global.get $__obj_slot_used)))",
            register_loop(0, 17)
        );
        if let Some((ok, out)) = run_driver(16, &over, "t") {
            assert!(!ok, "the 17th object of a 16-slot pool was silently accepted: {out}");
        }
    }

    /// While the shutdown walk runs (`__obj_no_reuse`), a freed handle is not handed out again
    /// and registration appends, which may GROW the pool with the free stack non-empty. The stack
    /// must move with it: once reuse is back on, the freed handles come back most recent first
    /// (11, then 7, then 3), not the garbage a stale stack would hold.
    #[test]
    fn handle_reuse_off_appends_and_the_free_stack_survives_a_grow() {
        let driver = format!(
            r#"(func $t (export "t") (result i64)
  (local $i i32) (local $a i32) (local $b i32) (local $c i32)
{first}  (call $__rt_handle_forget (i32.add (i32.const 4096) (i32.mul (i32.const 2) (i32.const 16))))
  (call $__rt_handle_forget (i32.add (i32.const 4096) (i32.mul (i32.const 6) (i32.const 16))))
  (call $__rt_handle_forget (i32.add (i32.const 4096) (i32.mul (i32.const 10) (i32.const 16))))
  (global.set $__obj_no_reuse (i32.const 1))
{more}  (global.set $__obj_no_reuse (i32.const 0))
  (local.set $a (call $__rt_handle_register (i32.const 9000)))
  (local.set $b (call $__rt_handle_register (i32.const 9016)))
  (local.set $c (call $__rt_handle_register (i32.const 9032)))
  (i64.add
    (i64.extend_i32_u (i32.add (i32.mul (local.get $a) (i32.const 10000))
                               (i32.add (i32.mul (local.get $b) (i32.const 100)) (local.get $c))))
    (i64.mul (i64.extend_i32_u (global.get $__obj_slot_used)) (i64.const 100000000))))"#,
            first = register_loop(0, 16),
            more = register_loop(16, 5)
        );
        if let Some((ok, out)) = run_driver(64, &driver, "t") {
            assert!(ok, "driver trapped");
            // 21 slots used (16 + 5 appended while reuse was off); reuse then answers 11, 7, 3.
            assert_eq!(out, "2100110703");
        }
    }
}
