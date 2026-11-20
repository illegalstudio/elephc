# Stored square-then-offset `array_map` results

## Design

- `StoredNumericMapShape` gained `SquareAffine { offset }`, parsed from
  `$v * $v + C`, `C + $v * $v`, and `$v * $v - C` (offset negation is checked).
  `C - $v * $v` is deliberately not folded because negating an `PHP_INT_MIN`
  square changes the overflow path.
- `__rt_array_map_square_affine` checks the square and then the offset addition in
  source order. The first promoted element releases the partial raw `array<int>`
  and rebuilds a boxed Mixed array with PHP's exact per-element int/float result.
- The marker's read-only consumer whitelist and all ownership/COW rules are shared
  with the numeric, affine, and square kernels.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 25 passed. Square-affine cases
  cover both addition orders, subtraction, square overflow, offset overflow,
  per-element types, element-write fallback, heap-debug cleanliness, and
  `--ir-opt` parity.
- `optimizer::`: 863 passed / 0 failed / 1 ignored.
- `arrays::`: 1298 passed / 0 failed / 1 ignored.
- `array_map_square_affine_emits_all_targets` unit test: 1 passed.
- Release x86_64 cross build + qemu matched PHP for `1692250000`, both addition
  orders, subtraction, and the square/offset overflow type fixture. The runtime
  object disassembly confirms the aligned `push %r15` prologue and no stack index.

## Benchmark

`benchmark/cases/native/array_map_square_affine` stores `$v * $v + 1` and sums it
5000 times; PHP and the C baseline both print `1692250000`.

- Formal harness (5 runs + 2 warmups, loaded host): elephc **1.753 ms** vs PHP
  30.819, PHP+JIT 18.497, C `-O2` 1.843. Neighbouring runs: square 2.913 vs 19.192
  / 22.340 / 0.676; affine 1.623 vs 18.303 / 18.171 / 0.839.
- Interleaved typed A/B against the parent compiler (`ffc6f17fe1`), in-process
  `hrtime`, 5000 maps of 100 ints: parent 22.389 / 22.533 / 22.565 / 22.678 /
  53.540 ms; new 0.706 / 0.709 / 0.712 / 0.723 / 0.756 ms. Both printed
  `1692250000` (median about 22.6 -> 0.712 ms, about 32x).
