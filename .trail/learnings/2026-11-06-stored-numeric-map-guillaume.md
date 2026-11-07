# Stored read-only `array_map` results without per-element Mixed cells

## Final design

- `src/ir_lower/stmt/stored_numeric_map.rs` marks a stored
  `$mapped = array_map(fn($v) => $v + C | $v - C | $v * C, $ints)` when the source is
  `array<int>`, the closure is capture-free and untyped, and every use of `$mapped` plus its
  value aliases is storage-neutral: indexed reads, value-`foreach`, read-only aliasing, or
  `count`/`sizeof`/`var_dump`/`print_r`/`implode`/`join`/`json_encode`/`array_sum`/
  `array_values`/`array_keys`/`array_key_exists`/`array_is_list`.
- The synthetic marker is consumed by `lower_function_call`; the checker's `array<mixed>`
  type is unchanged. The runtime kernel `__rt_array_map_numeric` builds a raw 8-byte
  `array<int>` while the literal operation cannot overflow; the first overflowing element
  releases that array and rebuilds an ordinary boxed Mixed array, preserving PHP's per-element
  int/float result.
- The raw representation is READ-ONLY. Any element write, direct (`$mapped[0] = ...`,
  `$mapped[] = ...`) or through a value alias, keeps the generic boxed map. This is the
  ownership/COW boundary: raw int storage never has to widen in place, and the established
  `array_set_mixed` / `array_push_refcounted` / COW paths remain untouched.

## Reverted experiments

- An intermediate version widened `array_push_refcounted` and `array_set_mixed`
  unconditionally. That broke nested-array literals (`[[7, 8], [9, 10]]` lost the first inner
  element), object-array pushes (`$keep[] = $e`), and several optimizer/GC tests. The final
  design reverts those runtime helpers to their original behavior and rejects every tracked
  element write at the marker pass instead.
- A guard that widened only non-empty raw scalar/tagged destinations still mishandled empty
  `array<never>`/object arrays. Keeping the runtime helpers untouched is both simpler and
  correct.

## Verification

- `tests/codegen/optimizer/stored_numeric_map.rs` (8): safe consumers, overflow int/float
  types, read-only copy, direct/aliased writes falling back, unsafe consumers falling back,
  operand orders, `--ir-opt` parity, and heap-debug cleanliness.
- `optimizer::` 837 passed / 0 failed / 1 ignored after the read-only design.
- `arrays::` 1298 passed / 0 failed / 1 ignored.
- Full `runtime_gc::` run: 1436 passed, 2 failed. One failure
  (`test_regression_escaped_caught_exception_retained_once`) was caused by the reverted push
  widening and passes after the revert. The other
  (`test_fileinfo_pointer_read_string_alias_owners`) is a pre-existing 24-byte string leak
  reproduced byte-for-byte with the parent compiler; it is unrelated to this change.

## Measurements

- A/B against the parent compiler from commit `ffc6f17fe1` (parent IR emits `runtime.array_map`,
  marked IR emits `runtime.array_map_numeric`), 5000 stored maps of 100 ints with `array_sum`,
  in-process hrtime and interleaved runs: parent 34.6 / 35.3 / 36.2 / 36.2 / 37.3 ms; new
  1.32 / 1.46 / 1.59 / 1.64 / 2.02 ms. Both printed `75750000` (about 20-26x).
- Preliminary formal benchmark under the concurrent `runtime_gc::` load (3 runs, 1 warmup):
  elephc 1.655 ms vs PHP 21.113, PHP+JIT 22.498, C -O2 0.504. A final 5/2 run is recorded
  after the suite finishes.
- Final A/B with the committed release (parent `ffc6f17fe1` vs `05145e31a3`), interleaved:
  parent 20.4 / 27.8 / 23.0 / 39.8 / 43.2 ms; new 1.85 / 1.01 / 1.67 / 0.828 / 3.35 ms.
  Both printed `75750000`; the parent IR has one `runtime.array_map`, the new IR has one
  `runtime.array_map_numeric` and no generic map. Median about 27.8 -> 1.67 ms (~17x) under
  the concurrent runtime_gc load.
- Final formal benchmark (5 runs, 2 warmups, same concurrent load): `array_map_stored_numeric`
  elephc 1.413 ms vs PHP 16.187, PHP+JIT 17.070, C -O2 0.643. Neighbouring rows:
  `array_map_foreach` 3.601 vs 17.869 / 16.889 / 1.007; `array_map_mul_reads` 1.902 vs
  15.931 / 15.918 / 0.469.
