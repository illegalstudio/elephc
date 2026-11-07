# Direct packed-int map sources

## Gap

The stored-map sink accepted variable sources, function-local variables, and all-int
array literals. A direct `array_map(fn($v) => $v + 1.5, range(1, 10))` or
`array_slice($values, 2, 3)` stayed generic.

## Change

- The marker now accepts a small, side-effect-free source grammar:
  - a variable (type proven at lowering);
  - a non-empty all-int array literal;
  - `range()` with two or three integer-literal arguments;
  - `array_slice()` with two to four arguments over another accepted source when
    the fourth argument is absent or the literal `false` (PHP's default key
    renumbering keeps a packed list).
- `lowering_packed_int_source` mirrors that grammar using the flow-sensitive
  `ctx.local_type` for variables (including variables nested in a slice) and the
  `functions` map to reject user shadows of `range`/`array_slice`. It runs before
  any emission, so a failed proof falls back to the generic `array_map` without
  evaluating the source twice.
- Any aliased local mentioned anywhere in the source expression disqualifies the
  candidate.
- Direct source temporaries (range arrays, slices) are released after the map call.
- `array_slice(..., preserve_keys: true)` stays generic because a slice that no
  longer starts at key 0 becomes a hash, which the packed kernels cannot read.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 55 passed (6 new). Covers direct
  `range()` (plain and stepped), `array_slice(range(...))`, nested slices, slices of
  variables and array literals, preserve-keys fallback, dynamic-range fallback,
  mixed-source fallback with a side-effecting offset evaluated once, heap-debug
  cleanliness, `--ir-opt` parity, and all-target assembly.
- PHP parity fixtures match byte-for-byte.
- Heap debug: `allocs == frees`, `live_blocks=0`.
- `optimizer::`: 911 passed / 0 failed / 1 ignored. The first concurrent run hit
  one environmental `Resource temporarily unavailable` spawn failure; the test
  passed alone and the uncontended rerun was clean.
- `arrays::`: 1301 passed / 0 failed / 1 ignored.
- Release x86_64 cross + qemu matched the direct, nested, heap, function-local,
  fallback, and both benchmark fixtures.

## Benchmarks

- `benchmark/cases/native/array_map_range_source`: direct `range(1, 100)` source,
  `$v + 1.5`, 5000 maps; PHP and C both print `26000000`.
- `benchmark/cases/native/array_map_slice_source`: direct
  `array_slice($values, 10, 80)`, 5000 maps; PHP and C both print `20800000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| direct `range(1, 100)` | **2.887 ms** | 11.644 | 12.909 | 0.762 |
| direct `array_slice(..., 10, 80)` | **1.393 ms** | 9.966 | 11.288 | 0.672 |
| function-local range (neighbour) | 1.294 | 11.102 | 11.994 | 2.121 |
| main-level float add (neighbour) | 2.603 | 11.354 | 12.226 | 0.729 |

Same-compiler A/B (in-process `hrtime`, 5000 maps of 100 ints):

- Direct range: parent `16def1a2cb` 15.761292 / 15.772625 / 15.835459 /
  15.988750 / 16.023000 / 16.088667 / 16.128125 ms, median about 15.989;
  same-compiler generic (dynamic range start) median about 15.668; fused
  0.852833 / 0.857209 / 0.863208 / 0.868375 / 0.870000 / 0.872500 / 0.874417
  ms, median about 0.868. Both printed `26000000`; about 18.4x vs parent and
  18.0x vs the generic baseline.
- Direct slice: parent 12.598833 / 12.793708 / 12.851958 / 12.888000 / 13.184917 /
  13.187458 / 13.204958 ms, median about 12.888; same-compiler generic
  (`$holder[0]` inner source) median about 12.765; fused 0.732209 / 0.742125 /
  0.747125 / 0.758292 / 0.761458 / 0.766792 / 0.786334 ms, median about 0.758.
  Both printed `20800000`; about 17.0x vs parent and 16.8x vs the generic
  baseline.
- EIR: parent and generic baselines use `runtime.array_map`; the fused direct
  sources use one `runtime.array_map_float_affine`.
- PHP: range 7.593 ms / JIT 6.567; slice 5.274 ms / JIT 5.378.
