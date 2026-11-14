# Raw integer counters for `$argc + constant` loop bounds

## Gap

Top-level benchmark loops use `for ($i = 0; $i < N + $argc; $i++)`. The
`$argc` local was already known to be a non-negative C `int`, but the checked
`N + $argc` bound could not be narrowed because it feeds a `PhpRelCmp` and the
boxed-narrowing pass blocked every comparison operand. The bound stayed a boxed
`ICheckedAdd`, the loop comparison stayed `PhpRelCmp`, and the counter stayed a
`Mixed` `str_inc_dec` cell. Every native loop paid that overhead per iteration.

## Change

- `main_argc_slot` now recognizes both the source `argc` local and the hidden
  `__elephc_func_argc#gen` parameter, and reads parameters when the slot is not a
  normal local.
- `boxed_narrowing::blocked_results` allows a `PhpRelCmp` with at least one
  `Mixed`/`TaggedScalar` partner to narrow its other operand. The comparison still
  has a runtime-tagged side, so its PHP dispatch table is unchanged.
- `IntegerRange` can therefore prove `N + $argc` (the non-negative C `int` range
  plus the constant), rewrite it to a raw `IAdd`, and the counter versioning pass
  promotes the induction counter to `I64` with an `ICmp` loop test.
- Regression test:
  `test_top_level_argc_bound_counter_is_integer` checks the optimized EIR and
  runs the loop with IR optimization on and off against PHP.

## Measurements

Body-only medians from `benchmark/frankenphp_body.py` (5 runs + 2 warmups):

| case | before | after | FrankenPHP | FP+JIT |
|---|---:|---:|---:|---:|
| `int_accumulator` | 36.748 | **7.670** | 61.497 | 15.343 |
| `mixed_int_division` | 61.379 | **17.781** | 40.349 | 33.359 |
| `sprintf_fixed` | 110.061 | **28.280** | 72.404 | 61.707 |
| `intval_string` | 253.053 | **75.619** | 74.430 | 50.027 |
| `json_encode_rows` | 615.969 | **204.351** | 186.634 | 193.922 |
| `mb_convert_kana` | 260.367 | **106.110** | 100.400 | 76.877 |
| `mb_decode_numericentity` | 186.276 | **64.748** | 58.596 | 47.589 |
| `mb_encode_numericentity` | 256.836 | **73.079** | 60.460 | 49.151 |
| `array_map_int_div_mixed` | 23.792 | **21.180** | 11.922 | 12.102 |
| `sum_loop` | 0.317 | 0.320 | 1.131 | 1.262 |

`int_accumulator`, `mixed_int_division` and `sprintf_fixed` move clearly ahead
of both FrankenPHP variants. The remaining behind cases are the mixed
int-division map rebuild, `intval_string`, the mbstring numeric-entity calls,
`mb_convert_kana` and the heavy JSON loop.

## Validation

- `loop_counter_checked_bounds`: 24 passed, including the new regression,
  top-level edge bounds (ascending, inclusive, zero-trip, descending), a
  reassigned-`$argc` semantic case, and the all-supported-target emission test.
- `bounded_mixed_counter`: 5 passed.
- Full `optimizer::` suite: **928 passed / 0 failed / 1 ignored**.
