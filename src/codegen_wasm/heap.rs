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
//!     H+0  i32 size      (usable payload bytes, header excluded: the BLOCK's capacity, a
//!                        multiple of 8, at least 8). A block taken from a larger free block is
//!                        split, so its size is exactly the rounded request; a block that was
//!                        not split can be larger than the request. Either way the word is the
//!                        capacity: `__rt_str_append` writes in place up to it, and nothing may
//!                        derive a value's SHAPE from it (an object's property count comes from
//!                        its class). Every block, free or live, is `size + 16` bytes, so the
//!                        cycle collector and the sweep can walk the heap linearly from
//!                        `__heap_base` to `__heap_ptr`.
//!     H+4  i32 refcount   (1 on allocation; 0 marks the block free)
//!     H+8  i64 kind       (type/GC metadata word; the allocator zeroes it on allocation, and a
//!                        free block carries `FREE_KIND` so the sweep can tell it from a block
//!                        that merely reached refcount 0 while its destructor runs)
//!   The user pointer is `H+16`. This is the SAME header `incref`/`decref` mutate.
//! - Free blocks live in segregated lists linked by header address (the `next` link is the
//!   first 4 payload bytes). A 608-byte table of list heads takes the first bytes of the region
//!   the caller passes as `heap_base` (the `__heap_base` global is the first block above it): one exact
//!   class per payload size 8..=1024 (O(1) pop and push), and 22 large bins keyed by the top
//!   bit of the payload, with `__heap_bins` as the occupancy mask. A request that misses its
//!   class takes from the large bins and splits off the tail; a large request scans only its
//!   own bin first-fit and otherwise takes the head of a higher bin.
//! - When nothing fits and at least a quarter of the heap has been freed since the last time,
//!   `__rt_heap_sweep` merges adjacent free blocks, rebuilds the lists, and returns a free tail
//!   to the bump region. `_gc_live`/`_gc_peak` count `size + 16` of live blocks as before.
//! - Allocation size, alignment, header, bump-pointer, and page calculations use
//!   widened i64 arithmetic. Command modules report deterministic PHP OOM;
//!   import-free reactor modules terminate through an explicit OOM trap helper.

use super::wat::{Global, ValType, WatModule};

/// Adds the heap/GC globals and the allocator routines to `wm`.
///
/// `heap_base` is the lowest address the heap owns (the 16-aligned offset just above the
/// runtime scratch + data segments): the size-class table takes its first `TABLE_BYTES`
/// bytes and the first block header follows; `heap_end` is one-past-end of the initially
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
    // The size-class table sits at `heap_base`; the first block follows it.
    let table = heap_base;
    let heap_base = heap_base + TABLE_BYTES;
    // Bump cursor + region bounds + large-bin occupancy mask.
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
    // Bit j set: large bin j (payloads whose top bit is 10 + j) is not empty.
    wm.add_global(Global {
        name: "__heap_bins".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    // Bytes freed since the last coalescing sweep (see `RT_HEAP_SWEEP`).
    wm.add_global(Global {
        name: "__heap_dirty".to_string(),
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
    wm.add_raw_func(&fill_table(RT_HEAP_PUSH, table));
    wm.add_raw_func(&fill_table(RT_HEAP_TAKE, table));
    wm.add_raw_func(&fill_table(RT_HEAP_SWEEP, table));
    wm.add_raw_func(RT_HEAP_ALLOC);
    wm.add_raw_func(&fill_table(RT_HEAP_FREE, table));
    wm.add_raw_func(RT_HEAP_FREE_SAFE);
}

/// Substitutes the table placeholders of an allocator routine: the table address, the address of
/// its large-bin half, the class limit, the free kind word and the table size.
fn fill_table(wat: &str, table: u32) -> String {
    wat.replace("{TBL_LARGE}", &(table + 4 * 129).to_string())
        .replace("{TBL}", &table.to_string())
        .replace("{SMALL_MAX}", &SMALL_MAX.to_string())
        .replace("{FREE_KIND}", &FREE_KIND.to_string())
        .replace("{TABLE_BYTES}", &TABLE_BYTES.to_string())
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

/// Bytes the size-class table occupies just below the first block: one 4-byte list head per
/// small class (slot 0 unused, slots 1..=128 for payloads 8..=1024 in steps of 8) and one per
/// large bin (22 bins, payloads `2^10 + 8 ..` split by the position of their top bit).
const TABLE_BYTES: u32 = 608;

/// Largest payload served from an exact-size class; anything bigger lives in a large bin.
const SMALL_MAX: u32 = 1024;

/// The kind word of a block on a free list. With refcount 0 it is what the sweep takes for
/// "free": a block between its last release and its free (refcount already 0, a destructor still
/// running) carries its real kind and is left alone.
const FREE_KIND: i64 = 0x7FEE_F4EE_7FEE_F4EE;

/// `__rt_heap_push`: marks the block at header `$hdr` free (refcount 0, free kind) and links it
/// at the head of the list for its size: the exact class for a payload up to 1024, otherwise the
/// large bin of its top bit (whose bit in `__heap_bins` is set).
const RT_HEAP_PUSH: &str = r#"(func $__rt_heap_push (param $hdr i32)
  (local $sz i32) (local $slot i32) (local $j i32)
  (local.set $sz (i32.load (local.get $hdr)))
  (i32.store (i32.add (local.get $hdr) (i32.const 4)) (i32.const 0))          ;; refcount = 0
  (i64.store (i32.add (local.get $hdr) (i32.const 8)) (i64.const {FREE_KIND})) ;; kind = free
  (if (i32.le_u (local.get $sz) (i32.const {SMALL_MAX}))
    (then
      (local.set $slot (i32.add (i32.const {TBL}) (i32.shr_u (local.get $sz) (i32.const 1)))))  ;; class = size / 8, 4 bytes each
    (else
      (local.set $j (i32.sub (i32.sub (i32.const 31) (i32.clz (local.get $sz))) (i32.const 10)))
      (local.set $slot (i32.add (i32.const {TBL_LARGE}) (i32.shl (local.get $j) (i32.const 2))))
      (global.set $__heap_bins (i32.or (global.get $__heap_bins) (i32.shl (i32.const 1) (local.get $j))))))
  (i32.store (i32.add (local.get $hdr) (i32.const 16)) (i32.load (local.get $slot))) ;; next = old head
  (i32.store (local.get $slot) (local.get $hdr)))
"#;

/// `__rt_heap_take`: claims a free block for `$size` payload bytes (already rounded to 8) and
/// returns its HEADER address, or 0 when no list holds one.
///
/// A payload up to 1024 pops its exact class; failing that (and for everything larger) it takes
/// from the large bins: the request's own bin is searched first-fit, every higher bin can only
/// hold bigger blocks and gives up its head. Claiming splits off the tail when at least a minimal
/// free block (16-byte header + 8 payload) remains, so the claimed block's size word is exactly
/// `$size` and the remainder is a well-formed free block that follows it in address order.
const RT_HEAP_TAKE: &str = r#"(func $__rt_heap_take (param $size i32) (result i32)
  (local $slot i32) (local $h i32) (local $prev i32) (local $cur i32) (local $nx i32)
  (local $m i32) (local $j i32) (local $rem i32)
  (if (i32.le_u (local.get $size) (i32.const {SMALL_MAX}))
    (then
      (local.set $slot (i32.add (i32.const {TBL}) (i32.shr_u (local.get $size) (i32.const 1))))
      (local.set $h (i32.load (local.get $slot)))
      (if (local.get $h)
        (then
          (i32.store (local.get $slot) (i32.load (i32.add (local.get $h) (i32.const 16))))
          (return (local.get $h))))                              ;; exact class hit: no split, no search
      (local.set $m (global.get $__heap_bins))
      (if (i32.eqz (local.get $m)) (then (return (i32.const 0))))
      (local.set $j (i32.ctz (local.get $m))))                   ;; any large block fits a small request
    (else
      (local.set $j (i32.sub (i32.sub (i32.const 31) (i32.clz (local.get $size))) (i32.const 10)))
      (if (i32.and (i32.shr_u (global.get $__heap_bins) (local.get $j)) (i32.const 1))
        (then
          (local.set $slot (i32.add (i32.const {TBL_LARGE}) (i32.shl (local.get $j) (i32.const 2))))
          (local.set $cur (i32.load (local.get $slot)))
          (block $found
            (loop $scan
              (br_if $found (i32.eqz (local.get $cur)))
              (if (i32.ge_u (i32.load (local.get $cur)) (local.get $size))
                (then
                  (local.set $nx (i32.load (i32.add (local.get $cur) (i32.const 16))))
                  (if (local.get $prev)
                    (then (i32.store (i32.add (local.get $prev) (i32.const 16)) (local.get $nx)))
                    (else
                      (i32.store (local.get $slot) (local.get $nx))
                      (if (i32.eqz (local.get $nx))
                        (then (global.set $__heap_bins (i32.and (global.get $__heap_bins)
                          (i32.xor (i32.shl (i32.const 1) (local.get $j)) (i32.const -1))))))))
                  (local.set $h (local.get $cur))
                  (br $found)))
              (local.set $prev (local.get $cur))
              (local.set $cur (i32.load (i32.add (local.get $cur) (i32.const 16))))
              (br $scan)))))
      (if (i32.eqz (local.get $h))
        (then
          (local.set $m (i32.and (global.get $__heap_bins)
            (i32.shl (i32.const -1) (i32.add (local.get $j) (i32.const 1)))))
          (if (i32.eqz (local.get $m)) (then (return (i32.const 0))))
          (local.set $j (i32.ctz (local.get $m)))))))
  (if (i32.eqz (local.get $h))
    (then
      ;; head of the lowest usable bin $j
      (local.set $slot (i32.add (i32.const {TBL_LARGE}) (i32.shl (local.get $j) (i32.const 2))))
      (local.set $h (i32.load (local.get $slot)))
      (local.set $nx (i32.load (i32.add (local.get $h) (i32.const 16))))
      (i32.store (local.get $slot) (local.get $nx))
      (if (i32.eqz (local.get $nx))
        (then (global.set $__heap_bins (i32.and (global.get $__heap_bins)
          (i32.xor (i32.shl (i32.const 1) (local.get $j)) (i32.const -1))))))))
  ;; split off the tail when a minimal free block remains
  (local.set $rem (i32.sub (i32.load (local.get $h)) (local.get $size)))
  (if (i32.ge_u (local.get $rem) (i32.const 24))
    (then
      (local.set $cur (i32.add (i32.add (local.get $h) (i32.const 16)) (local.get $size)))
      (i32.store (local.get $cur) (i32.sub (local.get $rem) (i32.const 16)))
      (i32.store (local.get $h) (local.get $size))
      (call $__rt_heap_push (local.get $cur))))
  (local.get $h))
"#;

/// `__rt_heap_sweep`: walks the heap block by block, merges every run of adjacent free blocks
/// into one, rebuilds all free lists from the merged runs, and, when the last block of the heap is
/// free, hands it back to the bump region. Runs only from the allocator, when a request found no
/// fit and at least a quarter of the heap has been freed since the previous sweep, so its cost is
/// paid for by the bytes freed. A header that cannot be a block (zero, unaligned or overrunning
/// size) stops the walk and keeps what was built: blocks past it simply stay off the lists.
const RT_HEAP_SWEEP: &str = r#"(func $__rt_heap_sweep
  (local $cur i32) (local $end i32) (local $sz i32) (local $nxt i32) (local $n i32)
  (memory.fill (i32.const {TBL}) (i32.const 0) (i32.const {TABLE_BYTES}))   ;; every list is rebuilt
  (global.set $__heap_bins (i32.const 0))
  (local.set $cur (global.get $__heap_base))
  (local.set $end (global.get $__heap_ptr))
  (block $done
    (loop $walk
      (br_if $done (i32.gt_u (i32.add (local.get $cur) (i32.const 24)) (local.get $end))) ;; no room for another block
      (local.set $sz (i32.load (local.get $cur)))
      (br_if $done (i32.or
        (i32.eqz (local.get $sz))
        (i32.or
          (i32.and (local.get $sz) (i32.const 7))
          (i64.gt_u
            (i64.add (i64.extend_i32_u (local.get $cur)) (i64.add (i64.extend_i32_u (local.get $sz)) (i64.const 16)))
            (i64.extend_i32_u (local.get $end))))))                ;; implausible header
      (local.set $nxt (i32.add (i32.add (local.get $cur) (i32.const 16)) (local.get $sz)))
      (if (i32.and
            (i32.eqz (i32.load (i32.add (local.get $cur) (i32.const 4))))
            (i64.eq (i64.load (i32.add (local.get $cur) (i32.const 8))) (i64.const {FREE_KIND})))
        (then
          (block $absorbed
            (loop $more
              (br_if $absorbed (i32.gt_u (i32.add (local.get $nxt) (i32.const 24)) (local.get $end)))
              (br_if $absorbed (i32.eqz (i32.and
                (i32.eqz (i32.load (i32.add (local.get $nxt) (i32.const 4))))
                (i64.eq (i64.load (i32.add (local.get $nxt) (i32.const 8))) (i64.const {FREE_KIND})))))
              (local.set $n (i32.load (local.get $nxt)))
              (br_if $absorbed (i32.or
                (i32.eqz (local.get $n))
                (i32.or
                  (i32.and (local.get $n) (i32.const 7))
                  (i64.gt_u
                    (i64.add (i64.extend_i32_u (local.get $nxt)) (i64.add (i64.extend_i32_u (local.get $n)) (i64.const 16)))
                    (i64.extend_i32_u (local.get $end))))))        ;; implausible neighbour: leave it
              (local.set $sz (i32.add (local.get $sz) (i32.add (local.get $n) (i32.const 16))))
              (local.set $nxt (i32.add (i32.add (local.get $nxt) (i32.const 16)) (local.get $n)))
              (br $more)))
          (i32.store (local.get $cur) (local.get $sz))             ;; the merged run's payload
          (if (i32.eq (local.get $nxt) (local.get $end))
            (then
              (global.set $__heap_ptr (local.get $cur))            ;; a free tail returns to the bump region
              (br $done)))
          (call $__rt_heap_push (local.get $cur))))
      (local.set $cur (local.get $nxt))
      (br $walk)))
  (global.set $__heap_dirty (i32.const 0)))
"#;

/// `__rt_heap_alloc`: returns the user pointer to a fresh block of at least `size`
/// bytes (8-byte minimum, rounded up to a multiple of 8) with refcount 1. Reuses a
/// free-list block (first fit) when one is large enough, otherwise bumps the heap
/// cursor, growing linear memory if the reserved region is exhausted.
const RT_HEAP_ALLOC: &str = r#"(func $__rt_heap_alloc (param $size i32) (result i32)
  (local $size64 i64) (local $need64 i64) (local $newend64 i64) (local $pages64 i64) (local $current_pages64 i64)
  (local $blk i32) (local $grow i32) (local $pages i32)
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
  ;; size-class lookup (split when larger); 0 when nothing fits
  (local.set $blk (call $__rt_heap_take (local.get $size)))
  ;; nothing fit: if a quarter of the heap has been freed since the last sweep, merge the
  ;; adjacent free blocks (and hand a free tail back to the bump region), then look once more
  (if (i32.eqz (local.get $blk))
    (then
      (if (i32.and
            (i32.ge_u (global.get $__heap_dirty) (i32.const 4096))
            (i32.ge_u
              (global.get $__heap_dirty)
              (i32.shr_u (i32.sub (global.get $__heap_ptr) (global.get $__heap_base)) (i32.const 2))))
        (then
          (call $__rt_heap_sweep)
          (local.set $blk (call $__rt_heap_take (local.get $size)))))))
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
  (local $hdr i32) (local $sz i32) (local $dirty i32)
  (if (i32.eq (local.get $ptr) (i32.const 0))                    ;; ignore null
    (then (return)))                                             ;; null -> ignore
  (local.set $hdr (i32.sub (local.get $ptr) (i32.const 16)))     ;; header address
  (local.set $sz (i32.load (local.get $hdr)))                    ;; payload size
  ;; accounting: live -= (size + 16), frees++
  (global.set $_gc_live (i64.sub (global.get $_gc_live) (i64.extend_i32_u (i32.add (local.get $sz) (i32.const 16))))) ;; live -= payload + header
  (global.set $_gc_frees (i64.add (global.get $_gc_frees) (i64.const 1))) ;; frees++
  (local.set $dirty (i32.add (global.get $__heap_dirty) (i32.add (local.get $sz) (i32.const 16))))
  (if (i32.lt_u (local.get $dirty) (global.get $__heap_dirty))
    (then (local.set $dirty (i32.const -1))))                    ;; saturate instead of wrapping
  (global.set $__heap_dirty (local.get $dirty))                  ;; freed since the last sweep
  (call $__rt_heap_push (local.get $hdr)))                       ;; refcount 0, free kind, onto its size list
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
    //!   skip with a SKIPPED line (fail under ELEPHC_TEST_REQUIRE_WASM_HOSTS) when `wasmer` is absent (validation always runs).

    use super::emit_heap_runtime;
    use super::super::wat::WatModule;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir() -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_heap_{}_{}", std::process::id(), n))
    }

    /// Returns whether the `wasmer` CLI is available; a missing host skips loudly or, under
/// `ELEPHC_TEST_REQUIRE_WASM_HOSTS`, fails (see `test_support`).
    fn wasmer_available() -> bool {
        crate::codegen_wasm::test_support::wasmer_available()
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

    /// WAT helper shared by the drivers below: walks the heap block by block exactly as the cycle
    /// collector does and answers 1 when the steps land exactly on `__heap_ptr` with every size a
    /// positive multiple of 8, else 0.
    const WALK: &str = r#"(func $walk (result i32)
  (local $h i32) (local $sz i32)
  (local.set $h (global.get $__heap_base))
  (block $done
    (loop $step
      (br_if $done (i32.ge_u (local.get $h) (global.get $__heap_ptr)))
      (local.set $sz (i32.load (local.get $h)))
      (if (i32.or (i32.eqz (local.get $sz)) (i32.and (local.get $sz) (i32.const 7)))
        (then (return (i32.const 0))))
      (local.set $h (i32.add (i32.add (local.get $h) (i32.const 16)) (local.get $sz)))
      (br $step)))
  (i32.eq (local.get $h) (global.get $__heap_ptr)))"#;

    /// Blocks of different small sizes live in different exact classes: a freed 24-byte block
    /// serves the next 24-byte request (flag 2) and is not handed to a 32-byte one (flag 1), so
    /// every small request is a pop of its own list, never a scan.
    #[test]
    fn small_sizes_use_separate_exact_classes() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32) (local $b i32) (local $c i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 24)))
  (drop (call $__rt_heap_alloc (i32.const 8)))
  (call $__rt_heap_free (local.get $a))
  (local.set $b (call $__rt_heap_alloc (i32.const 32)))
  (local.set $c (call $__rt_heap_alloc (i32.const 24)))
  (i32.add
    (i32.ne (local.get $b) (local.get $a))
    (i32.mul (i32.eq (local.get $c) (local.get $a)) (i32.const 2))))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "3");
        }
    }

    /// A small request that finds its class empty is carved out of a larger free block: the claimed
    /// block's size word is exactly the rounded request (flag 1, so it is a usable capacity, not
    /// the donor's), it starts where the donor did (flag 2), the remainder right behind it is a
    /// well-formed free block of the leftover size minus its header with refcount 0 (flag 4), and
    /// the linear heap walk is still valid (flag 8).
    #[test]
    fn small_request_splits_a_large_free_block() {
        let driver = format!(
            r#"(func $t (export "t") (result i32)
  (local $a i32) (local $b i32) (local $r i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 2000)))
  (drop (call $__rt_heap_alloc (i32.const 8)))
  (call $__rt_heap_free (local.get $a))
  (local.set $b (call $__rt_heap_alloc (i32.const 100)))
  (local.set $r (i32.add (local.get $b) (i32.const 104)))
  (i32.add
    (i32.add
      (i32.eq (i32.load (i32.sub (local.get $b) (i32.const 16))) (i32.const 104))
      (i32.mul (i32.eq (local.get $b) (local.get $a)) (i32.const 2)))
    (i32.add
      (i32.mul
        (i32.and
          (i32.eq (i32.load (local.get $r)) (i32.const 1880))
          (i32.eqz (i32.load (i32.add (local.get $r) (i32.const 4)))))
        (i32.const 4))
      (i32.mul (call $walk) (i32.const 8)))))
{WALK}"#,
            WALK = WALK
        );
        if let Some(o) = run_driver(3, &driver, "t") {
            assert_eq!(o, "15");
        }
    }

    /// A large request is served first-fit from its own bin and splits too: of two free large
    /// blocks (3000 and 1500) a 1200-byte request takes the one in its bin (1500), claims exactly
    /// 1200, and the walk stays valid.
    #[test]
    fn large_request_takes_its_own_bin_and_splits() {
        let driver = format!(
            r#"(func $t (export "t") (result i32)
  (local $a i32) (local $b i32) (local $c i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 3000)))
  (drop (call $__rt_heap_alloc (i32.const 8)))
  (local.set $b (call $__rt_heap_alloc (i32.const 1500)))
  (drop (call $__rt_heap_alloc (i32.const 8)))
  (call $__rt_heap_free (local.get $a))
  (call $__rt_heap_free (local.get $b))
  (local.set $c (call $__rt_heap_alloc (i32.const 1200)))
  (i32.add
    (i32.add
      (i32.eq (local.get $c) (local.get $b))
      (i32.mul (i32.eq (i32.load (i32.sub (local.get $c) (i32.const 16))) (i32.const 1200)) (i32.const 2)))
    (i32.mul (call $walk) (i32.const 4))))
{WALK}"#,
            WALK = WALK
        );
        if let Some(o) = run_driver(3, &driver, "t") {
            assert_eq!(o, "7");
        }
    }

    /// Freeing a run of adjacent small blocks and then asking for more than any of them is
    /// answered by merging the run: the request lands on the first block's address (it would
    /// otherwise bump past everything), and the walk stays valid.
    #[test]
    fn freed_neighbours_coalesce_for_a_bigger_request() {
        let driver = format!(
            r#"(func $t (export "t") (result i32)
  (local $first i32) (local $i i32) (local $p i32) (local $big i32)
  (local.set $first (call $__rt_heap_alloc (i32.const 64)))
  (local.set $i (i32.const 1))
  (loop $fill
    (drop (call $__rt_heap_alloc (i32.const 64)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $fill (i32.lt_u (local.get $i) (i32.const 100))))
  (drop (call $__rt_heap_alloc (i32.const 8)))
  (local.set $p (local.get $first))
  (local.set $i (i32.const 0))
  (loop $release
    (call $__rt_heap_free (local.get $p))
    (local.set $p (i32.add (local.get $p) (i32.const 80)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $release (i32.lt_u (local.get $i) (i32.const 100))))
  (local.set $big (call $__rt_heap_alloc (i32.const 4000)))
  (i32.add
    (i32.eq (local.get $big) (local.get $first))
    (i32.mul (call $walk) (i32.const 2))))
{WALK}"#,
            WALK = WALK
        );
        if let Some(o) = run_driver(3, &driver, "t") {
            assert_eq!(o, "3");
        }
    }

    /// When the freed run reaches the end of the heap, the sweep hands it back to the bump
    /// region: after freeing everything and asking for more than one block, the request starts at
    /// the heap's first block again.
    #[test]
    fn free_tail_returns_to_the_bump_region() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $first i32) (local $i i32) (local $p i32) (local $big i32)
  (local.set $first (call $__rt_heap_alloc (i32.const 64)))
  (local.set $i (i32.const 1))
  (loop $fill
    (drop (call $__rt_heap_alloc (i32.const 64)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $fill (i32.lt_u (local.get $i) (i32.const 100))))
  (local.set $p (local.get $first))
  (local.set $i (i32.const 0))
  (loop $release
    (call $__rt_heap_free (local.get $p))
    (local.set $p (i32.add (local.get $p) (i32.const 80)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $release (i32.lt_u (local.get $i) (i32.const 100))))
  (local.set $big (call $__rt_heap_alloc (i32.const 20000)))
  (i32.eq (local.get $big) (local.get $first)))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// A block whose refcount reached 0 but is not on a free list (a destructor still running on
    /// it) must never be merged by the sweep. Block B is stamped refcount 0 with a live kind (4);
    /// A and C around it are freed, and a request that forces a sweep must land past the guard
    /// block, leaving B's kind word untouched.
    #[test]
    fn sweep_leaves_a_block_mid_destruction_alone() {
        let driver = r#"(func $t (export "t") (result i32)
  (local $a i32) (local $b i32) (local $c i32) (local $d i32) (local $big i32)
  (local.set $a (call $__rt_heap_alloc (i32.const 2048)))
  (local.set $b (call $__rt_heap_alloc (i32.const 2048)))
  (local.set $c (call $__rt_heap_alloc (i32.const 2048)))
  (local.set $d (call $__rt_heap_alloc (i32.const 8)))
  (i64.store (i32.sub (local.get $b) (i32.const 8)) (i64.const 4))
  (i32.store (i32.sub (local.get $b) (i32.const 12)) (i32.const 0))
  (call $__rt_heap_free (local.get $a))
  (call $__rt_heap_free (local.get $c))
  (local.set $big (call $__rt_heap_alloc (i32.const 3000)))
  (i32.and
    (i32.gt_u (local.get $big) (local.get $d))
    (i64.eq (i64.load (i32.sub (local.get $b) (i32.const 8))) (i64.const 4))))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            assert_eq!(o, "1");
        }
    }

    /// `_gc_live` counts the claimed block's exact size plus header even when the block was
    /// carved from a bigger one, and returns to the guard block's share once it is freed.
    #[test]
    fn split_blocks_keep_the_live_accounting_exact() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $a i32) (local $b i32) (local $live i64)
  (local.set $a (call $__rt_heap_alloc (i32.const 2000)))
  (drop (call $__rt_heap_alloc (i32.const 8)))
  (call $__rt_heap_free (local.get $a))
  (local.set $b (call $__rt_heap_alloc (i32.const 100)))
  (local.set $live (global.get $_gc_live))
  (call $__rt_heap_free (local.get $b))
  (i64.add (i64.mul (local.get $live) (i64.const 1000)) (global.get $_gc_live)))"#;
        if let Some(o) = run_driver(3, driver, "t") {
            // live = (104 + 16) + (8 + 16) = 144 with the guard block; after freeing b: 24.
            assert_eq!(o, "144024");
        }
    }
}
