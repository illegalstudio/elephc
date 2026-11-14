# Stored square `array_map` results

## Design

- `StoredNumericMapShape` gained `Square`, parsed when both operands of the callback's
  multiplication are the closure parameter (`$v * $v`). The existing single-op and
  affine forms are unchanged.
- `__rt_array_map_square` checks one signed 64-bit product per element. A raw
  `array<int>` is built while every square fits; the first promoted element releases
  it and rebuilds a boxed Mixed array with PHP's exact per-element int/float result.
  Negative sources square to positive ints, matching PHP.
- The marker's read-only consumer whitelist and all ownership/COW rules are shared
  with the numeric/affine kernels: element writes and unsafe consumers keep the
  generic map.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 20 passed. Square cases cover
  negative values, neutral consumers, per-element promotion (including negative
  overflow), element-write fallback, heap-debug cleanliness, and `--ir-opt` parity.
- `array_map_square_emits_all_targets` unit test: 1 passed.
- `optimizer::`: 858 passed / 0 failed / 1 ignored.
- `arrays::`: 1298 passed / 0 failed / 1 ignored.
- Release x86_64 cross build + qemu matched PHP for `1691750000`, the negative square
  fixture, and the positive/negative overflow types.

## Benchmark

`benchmark/cases/native/array_map_square` stores `$v * $v` and sums it 5000 times;
PHP and the C baseline both print `1691750000`.

- Formal harness (5 runs + 2 warmups): elephc **1.302 ms** vs PHP 16.688,
  PHP+JIT 17.267, C `-O2` 0.685. Neighbouring runs: affine 1.495 vs 17.033 /
  16.710 / 0.628; stored numeric 1.533 vs 16.936 / 18.708 / 0.880.
- Interleaved typed A/B against the parent compiler (`ffc6f17fe1`), in-process
  `hrtime`, 5000 maps of 100 ints: parent 21.855 / 22.529 / 24.100 / 24.936 /
  34.165 ms; new 0.749 / 0.841 / 0.842 / 0.846 / 0.910 ms. Both printed
  `1691750000` (median about 24.1 -> 0.842 ms, about 29x).

## Full ownership suite

A fresh full `runtime_gc::` run after all three performance blocks (mb_chr, affine,
square) finished with **1437 passed / 1 failed**. The only failure is
`runtime_gc::pointer_read_owners::test_fileinfo_pointer_read_string_alias_owners`, the
pre-existing 24-byte string leak that reproduces byte-for-byte with the parent compiler
(recorded in `2026-11-06-stored-numeric-map-guillaume.md`).
