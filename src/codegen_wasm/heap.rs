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
//!     H+0  i32 size      (payload bytes, header excluded). This is the BLOCK's capacity,
//!                        not the caller's request: the free list reuses a larger block
//!                        without splitting it, so nothing may derive a value's SHAPE from
//!                        it — an object's property count comes from its class, never here.
//!     H+4  i32 refcount   (1 on allocation; 0 marks the block free)
//!     H+8  i64 kind       (type/GC metadata word; the allocator zeroes it)
//!   The user pointer is `H+16`. This is the SAME header `incref`/`decref` mutate.
//! - The free list is a singly-linked LIFO list of free blocks, linked by header
//!   address; a free block stores its `next` link at `H+16` (its payload's first
//!   4 bytes). This faithfully ports the native allocator's observable contract
//!   (header layout + `_gc_live`/`_gc_peak` accounting, which back PHP's
//!   `memory_get_usage()`); the native small-bins / coalescing are an intentional
//!   internal simplification with identical PHP-observable behavior.
//! - Allocation size, alignment, header, bump-pointer, and page calculations use
//!   widened i64 arithmetic. Command modules report deterministic PHP OOM;
//!   import-free reactor modules terminate through an explicit OOM trap helper.

use super::wat::{Global, ValType, WatModule};

/// Adds the heap/GC globals and the allocator routines to `wm`.
///
/// `heap_base` is the lowest header address (the 16-aligned offset just above the
/// runtime scratch + data segments); `heap_end` is one-past-end of the initially
/// reserved heap region (the top of the module's initial linear memory). The bump
/// allocator grows past `heap_end` with `memory.grow`.
pub(super) fn emit_heap_runtime(wm: &mut WatModule, heap_base: u32, heap_end: u32) {
    emit_heap_runtime_impl(wm, heap_base, heap_end, false);
}

/// Adds the heap runtime for a WASI command module with PHP-style OOM reporting.
pub(super) fn emit_command_heap_runtime(
    wm: &mut WatModule,
    heap_base: u32,
    heap_end: u32,
) {
    emit_heap_runtime_impl(wm, heap_base, heap_end, true);
}

/// Registers allocator globals and helpers with the selected OOM behavior.
fn emit_heap_runtime_impl(
    wm: &mut WatModule,
    heap_base: u32,
    heap_end: u32,
    command_runtime: bool,
) {
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
    if command_runtime {
        wm.add_raw_func(
            r#"(func $__rt_oom
  (call $__rt_fail (i32.const 6))
  unreachable ;; elephc-trap:deterministic-oom:command-oom
)"#,
        );
    } else {
        wm.add_raw_func(
            r#"(func $__rt_oom
  unreachable ;; elephc-trap:non-public:reactor-oom
)"#,
        );
    }
    wm.add_raw_func(RT_CHECKED_LAYOUT);
    wm.add_raw_func(RT_HEAP_ALLOC);
    wm.add_raw_func(RT_HEAP_FREE);
    wm.add_raw_func(RT_HEAP_FREE_SAFE);
}

/// `__rt_checked_layout`: validates `header + count * stride` entirely in
/// widened arithmetic and returns the representable wasm32 payload byte count.
///
/// The ceiling is the allocator's largest accepted payload request: it leaves
/// room for the 16-byte heap header, the lowest 1024-byte heap base used by the
/// runtime harness, and the final reserved wasm32 page. Invalid signed inputs,
/// an oversized header, or a count above the division-first bound take the same
/// deterministic OOM path as an allocator failure.
const RT_CHECKED_LAYOUT: &str = r#"(func $__rt_checked_layout (param $count i64) (param $stride i64) (param $header i64) (result i32)
  (local $remaining i64)
  (if (i64.lt_s (local.get $count) (i64.const 0))
    (then (call $__rt_oom) unreachable))                       ;; elephc-trap:deterministic-oom:layout-negative-count negative element count
  (if (i64.le_s (local.get $stride) (i64.const 0))
    (then (call $__rt_oom) unreachable))                       ;; elephc-trap:deterministic-oom:layout-nonpositive-stride zero/negative stride
  (if (i64.lt_s (local.get $header) (i64.const 0))
    (then (call $__rt_oom) unreachable))                       ;; elephc-trap:deterministic-oom:layout-negative-header negative header
  (if (i64.gt_u (local.get $header) (i64.const 4294900720))
    (then (call $__rt_oom) unreachable))                       ;; elephc-trap:deterministic-oom:layout-header-overflow header alone exceeds the payload ceiling
  (local.set $remaining
    (i64.sub (i64.const 4294900720) (local.get $header)))      ;; safe only after header <= ceiling
  (if (i64.gt_u
        (local.get $count)
        (i64.div_u (local.get $remaining) (local.get $stride)))
    (then (call $__rt_oom) unreachable))                       ;; elephc-trap:deterministic-oom:layout-size-overflow multiplication/addition would exceed wasm32
  (i32.wrap_i64
    (i64.add
      (local.get $header)
      (i64.mul (local.get $count) (local.get $stride)))))      ;; safe after the division-first bound
"#;

/// `__rt_heap_alloc`: returns the user pointer to a fresh block of at least `size`
/// bytes (8-byte minimum, rounded up to a multiple of 8) with refcount 1. Reuses a
/// free-list block (first fit) when one is large enough, otherwise bumps the heap
/// cursor, growing linear memory if the reserved region is exhausted.
const RT_HEAP_ALLOC: &str = r#"(func $__rt_heap_alloc (param $size i32) (result i32)
  (local $size64 i64) (local $need64 i64) (local $newend64 i64) (local $pages64 i64) (local $current_pages64 i64)
  (local $blk i32) (local $prev i32) (local $cur i32) (local $grow i32) (local $pages i32) (local $bsz i32)
  (local.set $size64 (i64.extend_i32_u (local.get $size)))     ;; preserve the full unsigned request
  ;; enforce minimum payload of 8 (a free block must hold an 8-byte next link)
  (if (i64.lt_u (local.get $size64) (i64.const 8))
    (then (local.set $size64 (i64.const 8))))                  ;; enforce minimum payload of 8
  (local.set $size64
    (i64.and (i64.add (local.get $size64) (i64.const 7)) (i64.const -8))) ;; checked-width alignment
  (local.set $need64 (i64.add (local.get $size64) (i64.const 16))) ;; header + aligned payload
  (if (i64.gt_u (local.get $need64) (i64.const 4294900736))
    (then
      (call $__rt_oom)
      unreachable))                                            ;; elephc-trap:deterministic-oom:heap-request-overflow request cannot fit below the wasm32 page cap
  (local.set $size (i32.wrap_i64 (local.get $size64)))         ;; safe after the widened bound check
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
      (local.set $newend64
        (i64.add (i64.extend_i32_u (global.get $__heap_ptr)) (local.get $need64))) ;; widened bump end
      (if (i64.gt_u (local.get $newend64) (i64.const 4294901760))
        (then
          (call $__rt_oom)
          unreachable))                                        ;; elephc-trap:deterministic-oom:heap-end-overflow keep the one-past-end pointer representable
      (if (i64.gt_u (local.get $newend64) (i64.extend_i32_u (global.get $__heap_end))) ;; would overrun the region
        (then
          (local.set $pages64
            (i64.shr_u (i64.add (local.get $newend64) (i64.const 65535)) (i64.const 16))) ;; required total pages
          (if (i64.gt_u (local.get $pages64) (i64.const 65535))
            (then
              (call $__rt_oom)
              unreachable))                                    ;; elephc-trap:deterministic-oom:heap-page-limit reserve a representable heap-end sentinel
          (local.set $current_pages64 (i64.extend_i32_u (memory.size))) ;; current total pages
          (local.set $pages
            (i32.wrap_i64 (i64.sub (local.get $pages64) (local.get $current_pages64)))) ;; additional pages
          (local.set $grow (memory.grow (local.get $pages)))     ;; grow linear memory
          (if (i32.eq (local.get $grow) (i32.const -1))
            (then
              (call $__rt_oom)
              unreachable))                                    ;; elephc-trap:deterministic-oom:memory-grow-failed host refused the requested pages
          (global.set $__heap_end
            (i32.shl (i32.wrap_i64 (local.get $pages64)) (i32.const 16))))) ;; exact grown region end
      (local.set $blk (global.get $__heap_ptr))                  ;; new block at the bump cursor
      (global.set $__heap_ptr (i32.wrap_i64 (local.get $newend64))) ;; advance after all checks succeeded
      (i32.store (local.get $blk) (local.get $size))))           ;; write header.size
  ;; claim: refcount = 1, kind = 0
  (i32.store (i32.add (local.get $blk) (i32.const 4)) (i32.const 1)) ;; refcount = 1
  (i64.store (i32.add (local.get $blk) (i32.const 8)) (i64.const 0)) ;; kind = 0
  ;; accounting: allocs++, live += (size + 16), peak = max(peak, live)
  (global.set $_gc_allocs (i64.add (global.get $_gc_allocs) (i64.const 1))) ;; allocs++
  (global.set $_gc_live (i64.add (global.get $_gc_live) (i64.add (i64.extend_i32_u (i32.load (local.get $blk))) (i64.const 16)))) ;; live += payload + header
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
  (local $size i32) (local $end i64)
  (if (i32.eq (local.get $ptr) (i32.const 0))                    ;; null -> ignore
    (then (return)))                                             ;; null -> ignore
  (if (i32.lt_u (local.get $ptr) (i32.add (global.get $__heap_base) (i32.const 16)))  ;; before the first payload
    (then (return)))                                             ;; before the heap -> ignore
  (if (i32.ge_u (local.get $ptr) (global.get $__heap_ptr))       ;; at/after the bump cursor (not live)
    (then (return)))                                             ;; beyond the bump cursor -> ignore
  (if (i32.ne (i32.and (local.get $ptr) (i32.const 7)) (i32.const 0)) ;; payload pointer must be 8-aligned
    (then (return)))                                             ;; misaligned pointer -> ignore
  (if (i32.eqz (i32.load (i32.sub (local.get $ptr) (i32.const 12))))  ;; refcount 0 -> already free
    (then (return)))                                             ;; already free -> ignore
  (local.set $size (i32.load (i32.sub (local.get $ptr) (i32.const 16)))) ;; payload size
  (if (i32.lt_u (local.get $size) (i32.const 8))                 ;; implausible header size
    (then (return)))                                             ;; implausible size -> ignore
  (if (i32.ne (i32.and (local.get $size) (i32.const 7)) (i32.const 0)) ;; payload size must be aligned
    (then (return)))                                             ;; malformed size -> ignore
  (local.set $end
    (i64.add (i64.extend_i32_u (local.get $ptr)) (i64.extend_i32_u (local.get $size)))) ;; full payload end
  (if (i64.gt_u (local.get $end) (i64.extend_i32_u (global.get $__heap_ptr)))
    (then (return)))                                             ;; block extends beyond the live heap window
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

    /// Builds and validates a reactor whose exported driver is expected to trap.
    fn driver_traps(pages: u32, driver: &str, export: &str) {
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
            return;
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
        assert!(!out.status.success(), "OOM driver unexpectedly succeeded\n{wat}");
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

    /// A maximal unsigned request must reach the explicit OOM path before any
    /// alignment, header, bump-pointer, or page arithmetic can wrap.
    #[test]
    fn oversized_request_traps_before_allocator_state_mutation() {
        let driver = r#"(func $t (export "t")
  (drop (call $__rt_heap_alloc (i32.const -1))))"#;
        driver_traps(1, driver, "t");
    }

    /// The checked-layout helper accepts both the zero-count/header-only case
    /// and the exact largest 8-byte-stride count without allocating the layout.
    #[test]
    fn checked_layout_accepts_exact_boundaries_without_allocating() {
        let driver = r#"(func $t (export "t") (result i64)
  (drop (call $__rt_checked_layout (i64.const 0) (i64.const 1) (i64.const 4294900720)))
  (i64.extend_i32_u
    (call $__rt_checked_layout
      (i64.const 536862587)
      (i64.const 8)
      (i64.const 24))))"#;
        if let Some(output) = run_driver(1, driver, "t") {
            assert_eq!(output, "4294900720");
        }
    }

    /// Every invalid checked-layout input traps before multiplication: one-past
    /// the exact count, negative operands, zero stride, and an oversized header.
    #[test]
    fn checked_layout_rejects_invalid_and_overflowing_boundaries() {
        let cases = [
            (536_862_588_i64, 8_i64, 24_i64),
            (-1, 8, 0),
            (1, 0, 0),
            (1, -1, 0),
            (1, 8, -1),
            (1, 1, 4_294_900_721),
            (i64::MAX, 4, 16),
        ];
        for (count, stride, header) in cases {
            let driver = format!(
                r#"(func $t (export "t")
  (drop (call $__rt_checked_layout
    (i64.const {count})
    (i64.const {stride})
    (i64.const {header}))))"#
            );
            driver_traps(1, &driver, "t");
        }
    }

    /// The safe free helper must reject a forged block whose declared payload
    /// extends beyond the live heap window, leaving its refcount unchanged.
    #[test]
    fn safe_free_checks_the_complete_payload_bounds() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $p i32)
  (local.set $p (call $__rt_heap_alloc (i32.const 16)))
  (i32.store (i32.sub (local.get $p) (i32.const 16)) (i32.const -8))
  (call $__rt_heap_free_safe (local.get $p))
  (i32.load (i32.sub (local.get $p) (i32.const 12))))"#;
        if let Some(output) = run_driver(2, driver, "t") {
            assert_eq!(output, "1");
        }
    }

    /// The safe free helper must reject a misaligned interior pointer without
    /// reading a fabricated header or altering the real allocation.
    #[test]
    fn safe_free_rejects_misaligned_interior_pointer() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $p i32)
  (local.set $p (call $__rt_heap_alloc (i32.const 16)))
  (call $__rt_heap_free_safe (i32.add (local.get $p) (i32.const 1)))
  (i32.load (i32.sub (local.get $p) (i32.const 12))))"#;
        if let Some(output) = run_driver(2, driver, "t") {
            assert_eq!(output, "1");
        }
    }
}
