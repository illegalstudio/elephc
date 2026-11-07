# Seeded raw `array_reduce()` products

## Gap

`array_reduce($ints, fn($c, $v) => $c * $v, C)` still used the generic boxed reduce:
one Mixed cell and one closure call per element, even when the source was a local
`array<int>` and `C` was a raw int/float.

## Change

- The shared `lower_reduce_seeded` proof now accepts `Add` and `Mul` bodies. The
  initial-value classifier is unchanged: literal or raw local/parameter int/float,
  with reference-bound locals rejected before lowering so fallback never evaluates
  the seed twice.
- Raw int seeds lower to `ArrayReduceProductSeed` → `__rt_array_product_int_i64`.
  The helper multiplies with `mul`/`smulh` (AArch64) or `imul` (x86_64). The first
  overflow converts both operands to double, multiplies them, and continues in
  double mode; only the final result is boxed.
- Raw float seeds lower to `ArrayReduceProductFloatSeed` →
  `__rt_array_product_float_i64`, which multiplies every element in double mode.
- Both helpers use only caller-saved registers and tail-call
  `__rt_mixed_from_value`; no stack frame and no per-element allocation. An empty
  range boxes the seed unchanged: int tag 0, float tag 2 (`-0.0` stays `float(-0)`).
- Both runtime functions are `BuiltinResultOwnership::Fresh`.

## Validation

- `tests/codegen/optimizer/reduce_product_seed.rs`: 6 passed. Covers both operand
  orders, int/float seeds, empty ranges, `PHP_INT_MAX * 2`,
  `PHP_INT_MIN * -1`, an overflow chain, zero products, `INF`, `-0.0`, unproven
  initials (string/bool/expression/reference) evaluated once, a non-mul body,
  heap-debug cleanliness, `--ir-opt` parity, and all-target assembly.
- PHP parity fixtures match byte-for-byte, including var_dump float formatting.
- x86_64 debug cross + qemu matched all four fixtures; the runtime object shows
  `imul %rax,%r11` / `jo promote` and `mulsd`.
- `array_product_int_emits_all_targets`: 1 passed.
- `optimizer::`: 892 passed / 0 failed / 1 ignored.
- `arrays::`: 1301 passed / 0 failed / 1 ignored.
- Focused `arrays::callbacks::test_array_reduce`: 15 passed / 0 failed.
- Sum-focused tests still pass after the shared-refactor: 12 passed. One old
  fallback assertion was updated because a `$c * $v` body now correctly takes the
  product fusion instead of the generic path.
- Release x86_64 cross + qemu matched PHP for all four fixtures, both benchmark
  mains (`36288000000`, `27216000000`), and the runtime object's `imul`/`jo`
  promotion sequence.
- EIR: `runtime.array_reduce_product_seed` / `runtime.array_reduce_product_float_seed`
  for the fused shapes; `runtime.array_reduce` only for the generic `$holder[0]`
  baselines.

## Benchmarks

- `benchmark/cases/native/array_reduce_product_seed`: 10 ints, seed 2, 5000
  reductions; PHP and C both print `36288000000`.
- `benchmark/cases/native/array_reduce_product_float_seed`: 10 ints, seed 1.5,
  5000 reductions; PHP and C both print `27216000000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| int seed 2 | **0.808 ms** | 6.669 | 8.091 | 0.630 |
| float seed 1.5 | **1.876 ms** | 6.658 | 7.657 | 0.595 |
| sum int seed (neighbour) | 0.878 | 12.219 | 13.511 | 0.700 |
| sum float seed (neighbour) | 1.118 | 12.629 | 13.511 | 2.071 |

Same-compiler A/B (`$values` fused vs `$holder[0]` generic boxed, in-process
`hrtime`, 5000 reductions):

- Int product: raw 0.221291 / 0.242917 / 0.247000 / 0.248333 / 0.256250 /
  0.281333 / 0.287083 ms; generic 6.629125 / 6.862916 / 6.870500 / 6.948542 /
  7.042375 / 7.063209 / 7.227083 ms. Both printed `36288000000` (median about
  6.949 -> 0.248 ms, about 28x).
- Float product: raw 0.216333 / 0.222667 / 0.223875 / 0.239916 / 0.252667 /
  0.254833 / 0.272125 ms; generic 7.015500 / 7.097416 / 7.567959 / 7.682833 /
  7.766875 / 7.909166 / 8.245000 ms. Both printed `27216000000` (median about
  7.683 -> 0.240 ms, about 32x).
- PHP on the same fixtures: int 1.325 ms / JIT 1.574; float 1.369 ms / JIT 1.486.
