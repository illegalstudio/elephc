# Mixed boxed division with exact int results

## Gap

`/` with a Mixed operand still coerced both sides to double, so `$a / $b` where
`$a = $argc + 5` and `$b = $argc + 2` produced `float(2)` instead of PHP's
`int(2)`. The checker had to keep Mixed division on the float path because the
runtime had no Mixed division helper.

## Change

- `MixedNumericOp` gained `Div`, and `mixed_numeric_op(BinOp::Div)` maps to it.
- The checker types Mixed/string divisions as `Mixed`, float operands as `Float`,
  and two plain ints as `Mixed` (literal divisions stay precise).
- `__rt_mixed_numeric_div` classifies both boxed operands:
  - int/bool/null/int-form numeric string → integer path;
  - any double or float-form numeric string → double path.
  - Two raw integer payloads take a direct fast path straight to the
    quotient/remainder rule, skipping the repeated unboxing casts.
  - The integer path casts both operands, checks a zero divisor, checks
    `PHP_INT_MIN / -1`, then uses quotient/remainder: exact and fitting → boxed
    int; otherwise both operands are divided as doubles.
  - The double path checks ±0.0 before `fdiv`.
  - A zero divisor returns a null pointer sentinel; valid boxed cells are never
    null.
- `lower_mixed_numeric_binop` checks the sentinel for Div, releases the operand
  boxes it created (and the temporary stack), then raises
  `DivisionByZeroError`. The normal path is unchanged.
- `stack_numeric` now admits division into the numeric-domain proof, and
  `stack_cell::lower_mixed` writes the quotient into the result's frame cell:
  - two integer payloads use `sdiv`/`idiv` plus the remainder check;
  - a float tag, an inexact quotient or `PHP_INT_MIN / -1` promotes both payloads
    and writes a double;
  - a zero integer or `±0.0` divisor raises `DivisionByZeroError` from the cell
    path before the cell is written.
- `checked_numeric_chain` explicitly excludes Div; only the frame-cell proof
  handles it.

## Validation

- `numeric_scalars`: Mixed division cases for exact/inexact ints, INT_MIN/-1,
  floats, numeric strings, null, bool, zero int/float divisors, heap cleanliness,
  and `--ir-opt` parity.
- `arrays::callbacks`: a Mixed divisor (`$argc + 1`) over `[1,2,3,4]` yields
  `float(0.5) int(1) float(1.5) int(2)`.
- All-target assembly checks `__rt_mixed_numeric_div` for the boxed fallback and
  `stack_numeric_div` for the frame-cell path.
- `test_mixed_division_cell_path_skips_heap_helper_on_all_targets`: a proven loop
  emits `stack_numeric_div` and no `__rt_mixed_numeric_div` on all five targets.
- `test_mixed_division_helper_path_survives_try_handlers`: a try handler falls
  back to the boxed helper.
- `test_mixed_division_cell_path_zero_divisors_raise`: integer and double zero
  divisors raise the catchable error.
- `test_mixed_division_cell_path_heap_debug_is_clean`.
- `test_mixed_division_cell_path_ir_opt_parity`: the frame-cell loop returns the
  same result with `--ir-opt=on` and `--ir-opt=off`.
- linux-x86_64 qemu matched PHP for every Mixed division case and both divisor
  kinds.
- `optimizer::`: 918 passed / 0 failed / 1 ignored.
- `arrays::`: 1303 passed / 0 failed / 1 ignored.
- Full release benchmarks and A/B are recorded below.

## Benchmark

`benchmark/cases/native/mixed_int_division`: `$a = $argc + 5`, `$b = $argc + 2`,
5,000,000 Mixed divisions; PHP and C both print `10000000`.

Formal harness (5 runs + 2 warmups):

| revision | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| parent `4a796584a9` (float path) | 88.404 ms | 59.759 | 49.545 | 0.395 |
| boxed helper only | 259.386 ms | 56.727 | 47.735 | 0.31 |
| helper int/int fast path | 326.389 ms | 55.704 | 55.243 | 0.503 |
| frame-cell quotient + accumulator | **40.351 ms** | 109.976 | 91.808 | 0.835 |

The parent formal run was on a lighter machine (C 0.395 ms) than the frame-cell
run (C 0.835 ms), yet the frame-cell path is still about 2.2x faster than the
parent and about 6.4x faster than the boxed helper. The loop assembly contains 15
`stack_numeric_div` labels and no `__rt_mixed_numeric_div` call.

Interleaved A/B on one core (5M iterations, in-process `hrtime`):

- parent `4a796584a9`: 80.507 / 123.035 / 143.706 / 218.575 / 232.212 /
  238.346 / 242.429 / 267.550 / 268.360 / 281.977 / 298.157 / 360.995 ms,
  median about 240 ms.
- new frame-cell path: 35.931 / 49.012 / 95.879 / 97.777 / 99.978 / 103.103 /
  103.489 / 106.249 / 107.411 / 113.762 / 124.451 / 126.014 ms, median about
  103 ms.
- PHP / JIT in the same batches: 125.693-235.945 ms / 127.898 ms.

Load average was 3.9-5.1 on 18 cores, so absolute figures are indicative; the
new path was faster than the parent in 11 of 12 interleaved runs. Both the
quotient box and the old `scvtf`/float-add path are gone, so exact division is
now cheaper than the previous float-only lowering on the common integer case.
