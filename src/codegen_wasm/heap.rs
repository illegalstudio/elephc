//! Purpose:
//! Emits the hand-authored WebAssembly (WAT) linear-memory allocator for the
//! wasm32-wasi backend: the heap/GC globals and the `__rt_heap_alloc` /
//! `__rt_heap_free` / `__rt_heap_free_safe` routines. This is the foundation for
//! every refcounted runtime value (strings, arrays, hashes, objects, Mixed).
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for every module (any program may allocate).
//!
//! Key details:
//! - All "pointers" are absolute i32 byte offsets into linear memory. Each block
//!   carries a 16-byte header immediately before the user pointer:
//!     H+0  i32 size      (payload bytes, the 8-rounded request, header excluded)
//!     H+4  i32 refcount   (1 on allocation; 0 marks the block free)
//!     H+8  i64 kind       (type/GC metadata word; the allocator zeroes it)
//!   The user pointer is `H+16`. This is the SAME header `incref`/`decref` mutate.
//! - The free list is a singly-linked LIFO list of free blocks, linked by header
//!   address; a free block stores its `next` link at `H+16` (its payload's first
//!   4 bytes). This faithfully ports the native allocator's observable contract
//!   (header layout + `_gc_live`/`_gc_peak` accounting, which back PHP's
//!   `memory_get_usage()`); the native small-bins / coalescing are an intentional
//!   internal simplification with identical PHP-observable behavior.
//! - The bump path grows linear memory with `memory.grow` when the reserved
//!   region is exhausted, and traps (`unreachable`) only at the wasm32 address
//!   limit, where the native runtime would print a fatal and exit.

use super::wat::{Global, ValType, WatModule};

/// Adds the heap/GC globals and the allocator routines to `wm`.
///
/// `heap_base` is the lowest header address (the 16-aligned offset just above the
/// runtime scratch + data segments); `heap_end` is one-past-end of the initially
/// reserved heap region (the top of the module's initial linear memory). The bump
/// allocator grows past `heap_end` with `memory.grow`.
pub(super) fn emit_heap_runtime(wm: &mut WatModule, heap_base: u32, heap_end: u32) {
    // Bump cursor + region bounds + free-list head.
    wm.add_global(Global {
        name: "__heap_base".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: heap_base as i64,
    });
    wm.add_global(Global {
        name: "__heap_ptr".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: heap_base as i64,
    });
    wm.add_global(Global {
        name: "__heap_end".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: heap_end as i64,
    });
    wm.add_global(Global {
        name: "__heap_free".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    // GC accounting counters (back PHP's memory_get_usage / memory_get_peak_usage).
    for counter in ["_gc_allocs", "_gc_frees", "_gc_live", "_gc_peak"] {
        wm.add_global(Global {
            name: counter.to_string(),
            ty: ValType::I64,
            mutable: true,
            init: 0,
        });
    }
    wm.add_raw_func(RT_HEAP_ALLOC);
    wm.add_raw_func(RT_HEAP_FREE);
    wm.add_raw_func(RT_HEAP_FREE_SAFE);
}

/// `__rt_heap_alloc`: returns the user pointer to a fresh block of at least `size`
/// bytes (8-byte minimum, rounded up to a multiple of 8) with refcount 1. Reuses a
/// free-list block (first fit) when one is large enough, otherwise bumps the heap
/// cursor, growing linear memory if the reserved region is exhausted.
const RT_HEAP_ALLOC: &str = r#"(func $__rt_heap_alloc (param $size i32) (result i32)
  (local $need i32) (local $blk i32) (local $prev i32) (local $cur i32) (local $grow i32) (local $newend i32) (local $pages i32) (local $bsz i32)
  ;; enforce minimum payload of 8 (a free block must hold an 8-byte next link)
  (if (i32.lt_u (local.get $size) (i32.const 8))
    (then (local.set $size (i32.const 8))))                      ;; enforce minimum payload of 8
  ;; round the payload up to a multiple of 8
  (local.set $size (i32.and (i32.add (local.get $size) (i32.const 7)) (i32.const -8))) ;; round payload up to a multiple of 8
  ;; free-list first-fit search; $blk stays 0 (its initial value) if nothing fits
  (local.set $prev (i32.const 0))                                ;; prev = null (free-list scan pointer)
  (local.set $cur (global.get $__heap_free))                     ;; cur = free-list head
  (block $break_search
    (loop $search
      (br_if $break_search (i32.eqz (local.get $cur)))           ;; end of list -> no fit found
      (local.set $bsz (i32.load (local.get $cur)))               ;; candidate block's payload size
      (if (i32.ge_u (local.get $bsz) (local.get $size))          ;; first block big enough wins
        (then
          (if (i32.eqz (local.get $prev))
            (then (global.set $__heap_free (i32.load (i32.add (local.get $cur) (i32.const 16)))))                  ;; unlink at head
            (else (i32.store (i32.add (local.get $prev) (i32.const 16)) (i32.load (i32.add (local.get $cur) (i32.const 16))))))  ;; unlink in middle
          (local.set $blk (local.get $cur))                      ;; claim this block
          (br $break_search)))                                   ;; found a fit -> stop scanning
      (local.set $prev (local.get $cur))                         ;; advance prev
      (local.set $cur (i32.load (i32.add (local.get $cur) (i32.const 16))))  ;; advance cur to next free block
      (br $search)))                                             ;; no fit yet -> next free block
  ;; bump fallback when no free block fit
  (if (i32.eqz (local.get $blk))
    (then
      (local.set $need (i32.add (local.get $size) (i32.const 16)))            ;; header + payload bytes
      (if (i32.gt_u (i32.add (global.get $__heap_ptr) (local.get $need)) (global.get $__heap_end))  ;; would overrun the region
        (then
          (local.set $newend (i32.add (global.get $__heap_ptr) (local.get $need))) ;; first address past the required bytes
          (local.set $pages (i32.div_u (i32.add (i32.sub (local.get $newend) (global.get $__heap_end)) (i32.const 65535)) (i32.const 65536)))  ;; pages to cover the shortfall
          (local.set $grow (memory.grow (local.get $pages)))     ;; grow linear memory
          (if (i32.eq (local.get $grow) (i32.const -1))
            (then (unreachable)))                                ;; at the wasm32 address limit -> abort
          (global.set $__heap_end (i32.add (global.get $__heap_end) (i32.mul (local.get $pages) (i32.const 65536)))))) ;; extend the reserved region
      (local.set $blk (global.get $__heap_ptr))                  ;; new block at the bump cursor
      (global.set $__heap_ptr (i32.add (local.get $blk) (local.get $need)))   ;; advance the bump cursor
      (i32.store (local.get $blk) (local.get $size))))           ;; write header.size
  ;; claim: refcount = 1, kind = 0
  (i32.store (i32.add (local.get $blk) (i32.const 4)) (i32.const 1)) ;; refcount = 1
  (i64.store (i32.add (local.get $blk) (i32.const 8)) (i64.const 0)) ;; kind = 0
  ;; accounting: allocs++, live += (size + 16), peak = max(peak, live)
  (global.set $_gc_allocs (i64.add (global.get $_gc_allocs) (i64.const 1))) ;; allocs++
  (global.set $_gc_live (i64.add (global.get $_gc_live) (i64.extend_i32_u (i32.add (i32.load (local.get $blk)) (i32.const 16))))) ;; live += payload + header
  (if (i64.gt_u (global.get $_gc_live) (global.get $_gc_peak))
    (then (global.set $_gc_peak (global.get $_gc_live))))        ;; peak = max(peak, live)
  (i32.add (local.get $blk) (i32.const 16)))                     ;; user pointer = header + 16
"#;

/// `__rt_heap_free`: returns the block behind a user pointer to the free list,
/// zeroing its refcount/kind and decrementing the live-bytes counter. A null
/// pointer is ignored.
const RT_HEAP_FREE: &str = r#"(func $__rt_heap_free (param $ptr i32)
  (local $hdr i32) (local $sz i32)
  (if (i32.eq (local.get $ptr) (i32.const 0))                    ;; ignore null
    (then (return)))                                             ;; null -> ignore
  (local.set $hdr (i32.sub (local.get $ptr) (i32.const 16)))     ;; header address
  (local.set $sz (i32.load (local.get $hdr)))                    ;; payload size
  ;; accounting: live -= (size + 16), frees++
  (global.set $_gc_live (i64.sub (global.get $_gc_live) (i64.extend_i32_u (i32.add (local.get $sz) (i32.const 16))))) ;; live -= payload + header
  (global.set $_gc_frees (i64.add (global.get $_gc_frees) (i64.const 1))) ;; frees++
  ;; mark free: refcount = 0, kind = 0
  (i32.store (i32.add (local.get $hdr) (i32.const 4)) (i32.const 0)) ;; refcount = 0
  (i64.store (i32.add (local.get $hdr) (i32.const 8)) (i64.const 0)) ;; kind = 0
  ;; push onto the free list (LIFO): this.next = old head, head = this
  (i32.store (i32.add (local.get $hdr) (i32.const 16)) (global.get $__heap_free)) ;; this block's next = old free-list head
  (global.set $__heap_free (local.get $hdr)))                    ;; head = this block
"#;

/// `__rt_heap_free_safe`: like `__rt_heap_free` but silently ignores a pointer
/// that is null, outside the live heap window, already free (refcount 0), or whose
/// header size is implausible. This lets speculative releases of borrowed/foreign/
/// data-segment/already-freed values be no-ops instead of corrupting the heap.
const RT_HEAP_FREE_SAFE: &str = r#"(func $__rt_heap_free_safe (param $ptr i32)
  (if (i32.eq (local.get $ptr) (i32.const 0))                    ;; null -> ignore
    (then (return)))                                             ;; null -> ignore
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))  ;; before the first payload
    (then (return)))                                             ;; before the heap -> ignore
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))       ;; at/after the bump cursor (not live)
    (then (return)))                                             ;; beyond the bump cursor -> ignore
  (if (i32.eqz (i32.load (i32.sub (local.get $ptr) (i32.const 12))))  ;; refcount 0 -> already free
    (then (return)))                                             ;; already free -> ignore
  (if (i32.lt_u (i32.load (i32.sub (local.get $ptr) (i32.const 16))) (i32.const 8))  ;; implausible header size
    (then (return)))                                             ;; implausible size -> ignore
  (call $__rt_heap_free (local.get $ptr)))                       ;; safe to free
"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT linear-memory allocator, exercised end-to-end under
    //! `wasmer` via a hand-written driver function and `--invoke`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds a minimal reactor module (memory + heap globals + the
    //!   allocator + one exported driver), validates it with `wasmparser`, and runs
    //!   the driver under `wasmer`, asserting the driver's returned value. The runs
    //!   skip silently when `wasmer` is absent (validation always runs).

    use super::emit_heap_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_heap_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a reactor module of `pages` linear-memory pages containing the heap
    /// runtime (base 1024, end = pages*64KB) plus `driver`, validates it, and runs
    /// `export` under `wasmer`, returning its trimmed stdout. `None` if wasmer is
    /// absent; validation always runs.
    fn run_driver(pages: u32, driver: &str, export: &str) -> Option<String> {
        let mut wm = WatModule::new();
        wm.set_memory(pages, Some("memory"));
        emit_heap_runtime(&mut wm, 1024, pages * 65536);
        wm.add_raw_func(driver);
        let wat = wm.render();
        let bytes = ::wat::parse_str(&wat)
            .unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        if !wasmer_available() {
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

    /// Two consecutive 24-byte allocations should be exactly `24 + 16 = 40` bytes
    /// apart, proving the header size and bump advance are correct.
    #[test]
    fn alloc_layout_is_contiguous() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32) (local $b i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 24)))
  (local.set $b (call $__rt_heap_alloc (i32.const 24)))
  (i32.sub (local.get $b) (local.get $a)))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "40");
        }
    }

    /// Allocating, freeing, then allocating the same size must reuse the exact same
    /// block (delta 0), proving the free-list push and first-fit reuse.
    #[test]
    fn free_then_alloc_reuses_block() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32) (local $b i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 16)))
  (call $__rt_heap_free (local.get $a))
  (local.set $b (call $__rt_heap_alloc (i32.const 16)))
  (i32.sub (local.get $b) (local.get $a)))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// A 20-byte request rounds the header size up to 24, and refcount is 1; the
    /// driver returns `size*100 + refcount = 2401`.
    #[test]
    fn header_size_rounds_and_refcount_is_one() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 20)))
  (i32.add
    (i32.mul (i32.load (i32.sub (local.get $a) (i32.const 16))) (i32.const 100))
    (i32.load (i32.sub (local.get $a) (i32.const 12)))))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "2401");
        }
    }

    /// Allocating then freeing a block must restore `_gc_live` to 0 (balanced
    /// accounting), which PHP's `memory_get_usage()` will report.
    #[test]
    fn gc_live_returns_to_zero_after_free() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 24)))
  (call $__rt_heap_free (local.get $a))
  (global.get $_gc_live))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "0");
        }
    }

    /// An allocation larger than the initial 1-page heap must trigger `memory.grow`
    /// and hand back usable memory: the driver stores 123 at the returned pointer
    /// and reads it back.
    #[test]
    fn large_alloc_grows_memory() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 70000)))
  (i32.store8 (local.get $a) (i32.const 123))
  (i32.load8_u (local.get $a)))"#;
        if let Some(o) = run_driver(1, driver, "t") {
            assert_eq!(o, "123");
        }
    }
}
