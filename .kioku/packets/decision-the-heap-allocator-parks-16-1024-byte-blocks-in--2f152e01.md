---
id: decision-the-heap-allocator-parks-16-1024-byte-blocks-in--2f152e01
type: decision
title: "The heap allocator parks 16-1024 byte blocks in exact size classes and defragments before exhaustion"
description: "Exact 16-byte classes replace the first-fit list walk for common sizes; parked blocks never coalesce, so one arena pass merges them before the allocator reports exhaustion"
created: 2026-09-27
sources:
  - path: src/codegen_support/runtime/arrays/heap_alloc.rs
    blob: 1ff93864001fcd19af7ea29c039615cc36325c5a
  - path: src/codegen_support/runtime/arrays/heap_free.rs
    blob: a6dae72f67333165451f2917e03d4114c9e022bb
---

# The heap allocator parks 16-1024 byte blocks in exact size classes and defragments before exhaustion

## Fact

Payloads 16..=HEAP_SIZE_CLASS_MAX (1024) go to one of 64 exact classes (`_heap_small_bins`, one head per class, index size/16-1); alloc pops, free pushes, split remainders that fit a class are parked there. The ordered coalescing list holds only larger blocks.

Invariant that removed the purge on tail free: a bump rewind passes only over the freed tail block and over ordered-list blocks during trim, so a parked class block can never sit above the bump. The alloc-side pop still validates bounds, free mark, kind and size as a backstop.

Parked blocks do not coalesce. `__rt_heap_alloc_defragment` (both arches) runs when the bump cannot fit a request AND some class is non-empty: it walks the arena header by header (the cycle collector's walk), merges runs of parked blocks (free mark + kind 0) into a rebuilt address-ordered list, returns a tail run to the bump, empties every class, and retries from the list. It cannot loop: a retry refills a class only by a successful split.

Tests: tests/codegen/runtime_gc/heap.rs test_gc_heap_defragments_size_class_blocks_before_exhausting (fails with "heap memory exhausted" if the pass is skipped) and test_gc_heap_parks_a_mid_sized_block_in_its_size_class (fails on the old allocator). The web request reset and the heap-debug validator both iterate HEAP_SIZE_CLASS_COUNT; a new reader of the classes must too.

## Why

__rt_heap_alloc was 13.6% of a Symfony request walking an address-ordered list full of small leftovers
