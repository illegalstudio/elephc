# Direct array_map indexed-read fusion

- Extended `src/ir_lower/stmt/map_read_fusion.rs` beyond the assigned temporary:
  `array_map(fn($v) => <pure arithmetic>, $ints)[$k]` now lowers directly to
  `isset($ints[$k]) ? <body with $v = $ints[$k]> : $ints[$k]` for a literal or simple
  variable key. The temporary is never materialized.
- The same closure rules apply: capture-free, untyped, pure `+`/`-`/`*` over the parameter
  and scalar literals. Typed returns, division/modulus, calls, captures, and unsupported
  index expressions keep the generic map.
- A missing key still falls through to the ordinary source read, preserving PHP's warning
  and null result without invoking the callback. Evaluating the source at the original
  expression position keeps earlier source writes observable.
- New tests in `tests/codegen/optimizer/array_map_read_fusion.rs`: literal/variable keys,
  nested array-literal traversal, missing-key warning/null, earlier source write, and
  typed/division rejection.
- Native benchmarks (release, 5 runs, 2 warmups, process timing):
  `array_map_direct_read` elephc 1.178 ms vs PHP 25.217, PHP+JIT 55.824, C -O2 1.808;
  `array_map_mul_reads` elephc 2.436 ms vs PHP 24.794, PHP+JIT 49.355, C -O2 0.894.
