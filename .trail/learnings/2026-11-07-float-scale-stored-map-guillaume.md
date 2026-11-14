# Stored float-literal `array_map` results

## Design

- `StoredNumericMapShape` gained `FloatScale { scale }`, parsed from `$v * F` and
  `F * $v` when the literal is an `ExprKind::FloatLiteral` (including a unary
  negation). Integer scales keep the existing int kernels, so `$v * 2.0` stays in
  the float domain while `$v * 2` stays an int map.
- `__rt_array_map_float_scale` converts each packed int element with `scvtf` /
  `cvtsi2sd`, multiplies by the literal scale, and stores an 8-byte IEEE-754 double
  into a raw `array<float>` (value_type 2). There is no boxed Mixed cell and no
  promotion branch: PHP's `$v * F` is always a float.
- `gettype($mapped[...])` is now a storage-neutral observation, so an element type
  probe no longer forces the generic map. Element writes and genuinely unsafe
  observers still fail closed.
- `array_sum()` over the raw float array now has a packed fast path in
  `array_numeric_aggregate.rs`: it checks value_type 2, sums the doubles with
  `fadd`/`addsd` in PHP's left-to-right order, and boxes only the final float. An
  empty raw float array returns int zero, matching PHP; `array_sum([-0.0])` was
  verified against PHP as positive `float(0)`.
- Pre-existing gap found while testing: the **generic** untyped float-callback map
  (`runtime.array_map` with `fn($v) => $v * 1.5` when the sink does not apply)
  crashes at run time; the compiler built before this change reproduces it. The new
  tests keep the positive cases on the raw kernel, and the element-write fallback
  is pinned in EIR instead of executing that unrelated path.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 31 passed. Float cases cover
  both operand orders, negative/identity/zero scales, large int conversion, JSON,
  direct read, `implode`, `array_sum`, heap-debug cleanliness, and `--ir-opt`
  parity.
- `array_map_float_scale_emits_all_targets` unit test: 1 passed.
- `optimizer::`: 869 passed / 0 failed / 1 ignored.
- `arrays::`: 1298 passed / 0 failed / 1 ignored.
- Release x86_64 cross build + qemu matched PHP for `37875000`, identity/zero
  scales, and the `-0.0` element/sum dump (`float(-0)` elements, positive
  `float(0)` sum).

## Benchmark

`benchmark/cases/native/array_map_float_scale` stores `$v * 1.5` and sums it 5000
times; PHP and the C baseline both print `37875000`.

- Formal harness (5 runs + 2 warmups, loaded host): elephc **3.872 ms** vs PHP
  25.215, PHP+JIT 30.750, C `-O2` 3.676. Before the raw-float aggregate fast path
  the same case measured 18.667 ms, so the consumer fast path is a 4.8x
  improvement on top of the map kernel.
- Construction-only A/B (`array_map` + `count`, in-process `hrtime`, 5000 maps of
  100 ints): parent `ffc6f17fe1` 7.152 / 7.198 / 7.253 / 7.525 / 7.558 ms; new
  0.576 / 0.581 / 0.584 / 0.597 / 0.639 ms. Both printed `500000` (median about
  7.25 -> 0.584 ms, about 12.4x). PHP was 12.983 ms.
- The end-to-end `array_sum` fixture cannot be A/B-tested against the parent: the
  parent SIGSEGVs on the generic untyped float-callback path, reproduced with the
  compiler built before this change. The new stored kernel runs it successfully.
