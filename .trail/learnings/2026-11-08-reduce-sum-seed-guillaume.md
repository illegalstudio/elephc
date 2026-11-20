# Seeded raw integer `array_reduce()` sum

## Gap

`array_reduce($ints, fn($c, $v) => $c + $v, 0)` already fused to `array_sum()`, and a
typed `fn(int $c, int $v): int` reducer used `__rt_array_reduce_int_direct`. The
untyped reducer with a **nonzero literal integer initial** still took the generic
boxed reduce, allocating one Mixed cell per element and calling the closure.

## Change

- `lower_reduce_sum` now accepts any literal integer initial (including unary
  negation). Zero keeps `ArraySum`; any other literal lowers to the new
  `ArrayReduceSumSeed` runtime function, which calls `__rt_array_sum_int_i64` with
  the literal seed. The carry stays raw until the first overflow, then continues as
  a double, and only the final result is boxed.
- `reduce_sum_seed.rs` materializes the packed-array helper call for each target:
  `x0`/`rdi` = array, start 0, end = array length, `x3`/`rcx` = raw seed.
- `ArrayReduceSumSeed` is classified `BuiltinResultOwnership::Fresh`: the helper always
  boxes a new int-or-float cell and never returns storage from the source array.
- The fusion still requires a local `array<int>` source, an untyped capture-free
  two-parameter closure whose body is `$c + $v` or `$v + $c`, and a literal integer
  initial. Everything else (other bodies, captures, typed params/returns, float or
  non-literal initial, inline array literals) keeps the generic boxed reduce.

## Validation

- `tests/codegen/optimizer/reduce_sum_seed.rs`: 5 passed. Covers both operand
  orders, negative and empty seeds, overflow promotion, generic-body fallback,
  heap-debug cleanliness, `--ir-opt` parity, and an all-target assembly check that
  the seeded reducer call is present and the generic boxed reduce is not.
- PHP parity matrix (seeds 10/-3/7, `PHP_INT_MAX+1`, `PHP_INT_MIN-1`, float initial,
  empty array): byte-identical output.
- EIR: `runtime.array_reduce_sum_seed`, no `runtime.array_reduce` for the fused
  shape. Assembly calls `__rt_array_sum_int_i64` with `mov x3, #10` and no closure.
- x86_64 cross build + qemu matched PHP for all fixtures (`15180`, the full matrix,
  `25300000`).
- `optimizer::`: 878 passed / 0 failed / 1 ignored, rerun after the `Fresh` ownership
  classification.
- `arrays::`: 1301 passed / 0 failed / 1 ignored; focused
  `arrays::callbacks::test_array_reduce` 15 passed / 0 failed.
- Release x86_64 cross + qemu matched PHP for `15180`, the full overflow/float matrix,
  and `25300000`.

## Benchmark

`benchmark/cases/native/array_reduce_seed` reduces 100 ints with seed 10, 5000
times; PHP and C `-O2` both print `25300000`.

- Formal harness (5 runs + 2 warmups): elephc **1.519 ms** vs PHP 24.564,
  PHP+JIT 26.569, C `-O2` 3.340.
- Same-compiler A/B (`$values` raw kernel vs `$holder[0]` generic boxed reduce,
  in-process `hrtime`, 5000 reductions): raw 0.398 / 0.404 / 0.404 / 0.418 /
  0.428 / 0.430 / 0.614 ms; generic 95.857 / 96.268 / 153.589 / 159.870 /
  167.959 / 180.511 / 221.518 ms. Both printed `25300000` (median about
  159.87 -> 0.418 ms, about 382x). A first round with a single noisy raw sample
  measured 0.399-0.951 ms raw vs 92.33-191.65 ms generic.
- EIR for the generic baseline is `runtime.array_reduce` (boxed callback path);
  the fused shape is one `runtime.array_reduce_sum_seed` call and no closure.
  PHP on the same fixture: 34.664 ms; PHP+JIT: 13.324 ms.
