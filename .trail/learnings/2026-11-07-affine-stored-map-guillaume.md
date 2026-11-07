# Stored affine `array_map` results

## Design

- `src/ir_lower/stmt/stored_numeric_map.rs` now owns a shared
  `parse_stored_numeric_map(body, param)` shape parser. The original single-op forms
  (`$v + C`, `$v - C`, `$v * C`, including `C + $v` / `C * $v`) still lower to
  `ArrayMapNumeric`; composite `$v * A + B` forms lower to the new `ArrayMapAffine`.
- Affine forms accepted: `$v * A + B`, `$v * A - B`, `B + $v * A`, `B - $v * A`,
  and literal negation (`$v * -1 + 3`). The body remains capture-free, untyped, and a
  single `return`.
- `__rt_array_map_affine` (`src/codegen_support/runtime/arrays/array_map_affine.rs`)
  checks the multiply and then the add in source order. The first promoted element
  releases the partial raw `array<int>` and rebuilds a boxed Mixed array with PHP's
  exact per-element int/float result. The safe result stays a raw 8-byte `array<int>`.
- The marker's read-only consumer whitelist is unchanged: indexed reads,
  value-`foreach`, read-only aliasing, `count`, `array_sum`, `implode`, `json_encode`,
  `var_dump`, `array_values`, `array_keys`, etc. Element writes and consumers that may
  assume boxed Mixed slots keep the generic map.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 15 passed. The affine cases cover
  all operand orders, positive/negative multiply and add overflow, per-element
  int/float types, neutral consumers, element-write fallback, unsafe-consumer
  fallback, heap-debug cleanliness, and `--ir-opt` parity.
- `array_map_affine_emits_all_targets` unit test: 1 passed.
- x86_64 cross build + `qemu-x86_64 -L /usr/x86_64-linux-gnu` matched PHP for the
  basic, positive-overflow, and negative-overflow fixtures.
- `optimizer::`: 853 passed / 0 failed / 1 ignored.
- `arrays::`: 1298 passed / 0 failed / 1 ignored.
- Release x86_64 cross build + qemu matched PHP for `76250000`, all four basic
  operand orders, and the positive/negative overflow type fixture.

## Benchmark

`benchmark/cases/native/array_map_affine` stores `$v * 3 + 1` and sums it 5000 times;
PHP and the C baseline both print `76250000`.

- Formal harness (5 runs + 2 warmups, loaded host): elephc **4.153 ms** vs PHP 27.004,
  PHP+JIT 26.146, C `-O2` 2.430. The neighbouring stored-numeric run measured
  3.540 vs 29.821 / 28.647 / 1.265 under the same load.
- Interleaved typed A/B against the parent compiler (`ffc6f17fe1`), in-process
  `hrtime`, 5000 maps of 100 ints: parent 38.75 / 40.88 / 94.31 / 99.66 / 39.66 ms;
  new 1.215 / 1.238 / 4.291 / 1.596 / 1.204 ms. Both printed `76250000`
  (median about 40.9 -> 1.24 ms, about 33x).
