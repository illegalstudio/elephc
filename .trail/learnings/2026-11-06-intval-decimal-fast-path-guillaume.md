# Restored direct decimal intval/(int) parsing and base-10 forwarding

- `__rt_str_to_int` now handles the common plain-decimal run without `__rt_cstr`, `strtoll`,
  or `strtod`: optional PHP whitespace and sign, then up to 18 digits accumulated in place.
  Eighteen digits always fit i64, so the hot path needs no saturation constants; a 19th digit
  or a `.`/`e`/`E` continuation bails to the general parser, which owns saturation and PHP's
  float-form cap. Any other terminator returns the exact integer prefix, matching
  `intval("42abc")`, `"1_000"`, `"0x1A"`, and `"  -42xyz"`.
- `__rt_str_to_int_base` forwards base 10 to the general parser. PHP applies numeric-string
  float rules only at base 10 (`intval("1e3", 10) === 1000`, `intval("9.9e18", 10)` caps at
  `PHP_INT_MAX`), while other bases keep digit-only `strtol` parsing
  (`intval("1e3", 16) === 483`).
- Fixed the x86_64 classifier to preserve the source byte until after the digit range check;
  it previously compared `.`/`e` after subtracting 48 and silently returned the integer prefix.
- Verified against PHP 8.5 on aarch64 and linux-x86_64/qemu across whitespace, plus/minus,
  leading zeros, underscore/hex terminators, i64 saturation, NUL bytes, empty runs, `.5`,
  `1e3`, `1.5e2`, `9.9999999999999999e18`, and `...e-999`.
- New regression: `tests/codegen/scalar_strings.rs::test_intval_runtime_decimal_fast_path_boundaries`.
  Native benchmark `benchmark/cases/native/intval_string` (18-digit and partial strings, 3M
  XOR-parses, process timing, 3 runs): elephc 59.17 ms vs PHP 54.54, PHP+JIT 42.25, C -O2
  24.80. The same fixture was 169.86 ms before the in-place parser. A comparable
  `intval(int)` loop is already 54 ms, so the remaining gap is loop/codegen overhead rather
  than string parsing.
- Focused: `intval` 16 passed (GMP link test skipped: missing native GMP), `scalar_strings`
  27 passed, casts 33 passed.
