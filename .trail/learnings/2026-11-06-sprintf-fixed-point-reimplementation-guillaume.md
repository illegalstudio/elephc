# Restored sprintf exact fixed-point `%f`/`%F`

- Reimplemented `emit_float_fixed_fast_path` (AArch64) and
  `emit_float_fixed_fast_path_x64` after the `__rt_sprintf_flt_nz` labels. Both
  accept only `%f`/`%F` with no sign/left/alternate flag and precision at most
  nine; when `value * 10^precision` is exactly an integer below `2^52`, the
  scaled integer is PHP's correctly rounded body and is formatted with
  `__rt_itoa_into` into the shared padding/copy stage. Fractional/oversized
  values, NaN, infinity, and wider precisions keep `snprintf`.
- Boundary matrix matches PHP 8.5 on AArch64 and on linux-x86_64 under
  `qemu-x86_64`: exact short decimals, `2.675`/`1.005` fallbacks, `1e9`, `1e18`,
  `%.0f` rounding, `%F`, default precision, zero/space padding, `%.9f`,
  `%.10f`, and `%.30f`.
- Native benchmark `benchmark/cases/native/sprintf_fixed` (500k
  `%d:%s:%.2f` calls, process timing, 3 runs): elephc 32.17 ms, PHP 47.33,
  PHP+JIT 46.01, C -O2 28.11. Beats PHP+JIT by 1.43x; remains 0.87x of C.
- Focused: `strings::formatting` 58 passed, `sprintf` 56 passed/1 ignored,
  `printf` 66 passed/1 ignored.
