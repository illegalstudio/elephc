# PHP-exact int/int division and the integer-divisor stored map

## Gap

The checker typed every `/` as `float`, so an exact `6 / 3` produced a double
instead of PHP's `int(2)`, and the generic `array_map(fn($v) => $v / 2, ...)`
callback boxed every exact quotient as a double. The stored-map sink also left
`$v / C` on the generic path.

## Change

- The checker now distinguishes plain int operands from float/Mixed ones:
  - `int / int` is `Mixed` unless both sides are literals, where an exact quotient
    stays `Int`, an inexact one is `Float`, and `PHP_INT_MIN / -1` is `Float`.
  - A Mixed/string operand keeps the established float path; the compiler still
    lacks a Mixed `/` helper that preserves run-time exactness.
- EIR gained `ICheckedDiv`. Codegen lowers it with:
  - a `DivisionByZeroError` guard for a zero divisor;
  - a `PHP_INT_MIN / -1` guard that takes the promoted double path without
    trapping `idiv`;
  - a quotient/remainder check: remainder zero boxes the integer quotient,
    otherwise both operands are divided as doubles.
  The x86 path preserves the original dividend in `r8` across `idiv`, and the
  AArch64 path keeps the divisor in `x10` while using `x11` for the remainder.
- The AST stored-map shape gained `IntDiv { divisor }` for a nonzero integer
  literal. `__rt_array_map_int_div` builds a raw `array<int>` while every quotient
  is exact, then releases it and rebuilds a boxed Mixed array at the first
  inexact element or `PHP_INT_MIN / -1`.
- A zero divisor still keeps the generic map (`DivisionByZeroError`); a float
  divisor uses the existing raw float-division kernel.

## Validation

- `numeric_scalars`: direct exact/inexact division, `PHP_INT_MIN / -1`, `/1`,
  negative quotients, and the zero-divisor catch.
- `arrays::callbacks`: untyped `array_map(fn($v) => $v / 2, ...)` preserves
  `float(0.5) int(1) float(1.5) int(2)`.
- `stored_numeric_map`: 61 passed (6 new): exact raw results, Mixed rebuild,
  negative divisors, INT_MIN edges, zero/empty fallbacks, heap debug, `--ir-opt`
  parity, and all-target assembly.
- x86_64 qemu matched PHP for runtime-variable division, the kernel's INT_MIN
  edges, the inline generic map fixture, and both benchmark mains.
- `optimizer::`: 917 passed / 0 failed / 1 ignored.
- `arrays::`: 1302 passed / 0 failed / 1 ignored.
- Release x86_64 cross + qemu matched the direct, kernel, and benchmark fixtures.

## Benchmarks

- `benchmark/cases/native/array_map_int_div`: even values 2..200 divided by 2,
  5000 maps; PHP and C both print `25250000`.
- `benchmark/cases/native/array_map_int_div_mixed`: values 1..100 divided by 2,
  mixed exact/inexact results; PHP and C both print `12625000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| exact even values / 2 | **1.209 ms** | 17.531 | 16.705 | 1.710 |
| mixed values / 2 | 25.123 ms | 18.668 | 20.769 | 1.531 |

Same-compiler A/B (in-process `hrtime`, 5000 maps):

- Exact: parent `99ec83883f` 22.665333 / 22.717000 / 22.861959 / 23.006083 /
  23.022292 / 23.273541 / 24.189417 ms, median about 23.006; fused 0.854958 /
  0.884750 / 0.909167 / 0.922375 / 0.927958 / 0.932000 / 0.940250 ms, median
  about 0.922. Both printed `25250000`; about 25x vs the parent.
- Mixed: parent 23.118500 / 23.215209 / 23.247917 / 23.734875 / 25.058625 /
  25.233791 / 25.781583 ms, median about 23.735; fused 21.761667 / 21.848500 /
  22.101417 / 23.297167 / 23.666583 / 23.722250 / 24.625208 ms, median about
  23.297. Both printed `12625000`; roughly parity, because the Mixed rebuild boxes
  one cell per inexact element. The kernel still avoids closure calls and is the
  correctness path for mixed results.
- A dynamic Mixed divisor forces the generic path (about 66-89 ms), but that is
  not a storage-only baseline: it also boxes the divisor operand.
- EIR: the parent and generic baseline use `runtime.array_map`; the fused exact
  and mixed fixtures use one `runtime.array_map_int_div`.
- PHP: exact 11.500 ms / JIT 12.003; mixed 11.689 ms / JIT 12.272.
