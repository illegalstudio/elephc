# Restored pure array_map indexed-read fusion and count sink

- Recreated `src/ir_lower/stmt/map_read_fusion.rs`, called from
  `lower_body_into_function` before EIR lowering (skipped in functions with
  dynamic eval). It rewrites a materialized `$mapped = array_map(fn($v) =>
  <pure arithmetic>, $ints)` temporary used only by constant-indexed reads:
  each read becomes `isset($ints[$k]) ? <body with $v = $ints[$k]> : $ints[$k]`,
  and the map assignment is removed.
- The closure must be capture-free, untyped, and a pure `+`/`-`/`*` tree over
  its parameter and scalar literals. Typed returns, division/modulus, calls,
  captures, source writes after the map, aliases, uses in another block, and
  any unsupported expression fail closed and keep the generic map. A missing
  key keeps the source warning and null result.
- Recreated `lower_count_map` in `static_array_callbacks.rs`: for the same pure
  closures, `count(array_map(...))` lowers to `ArrayLen(source)` with no map.
  Capturing, typed, or effectful callbacks keep the map and its per-element
  calls.
- Tests: `tests/codegen/optimizer/array_map_read_fusion.rs` (9 tests) and
  `array_map_count_fusion.rs` (3 tests), covering Add/Sub/Mul, compound
  arithmetic, overflow promotion, missing-key warnings, source writes,
  typed/capturing/division rejections, and block escapes.
- Regressions: `optimizer::` 808 passed / 0 failed / 1 ignored; `array_map`
  102 passed; `arrays::callbacks` 132 passed; `cargo check --tests` clean.
- Native benchmark `benchmark/cases/native/array_map_mul_reads` (5000 rounds,
  three reads): release elephc 1.284 ms vs PHP 10.832, PHP+JIT 10.848, C -O2
  0.217. The fusion is 8.4x faster than PHP+JIT, but C still wins because GCC
  hoists the three loop-invariant reads to constants while the fused loop keeps
  three guarded reads per iteration. Hoisting those guarded reads is the next
  optimization target.
