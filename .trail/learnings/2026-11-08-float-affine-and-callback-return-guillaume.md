# Generic float-callback return bridge and stored float-affine maps

## Generic untyped float-callback SIGSEGV

- Root cause: `try_emit_direct_closure_wrapper` checked only that the map result channel was
  Mixed. When the closure parameter already matched the source element it tail-called the
  closure directly, but the closure's EIR return was a raw `F64` in `d0`/`xmm0`. The Mixed map
  helper read `x0` as an owned Mixed pointer, so `array_sum(array_map(fn($v) => $v * 1.5, ...))`
  and element writes crashed.
- Fix: pass the closure's actual `return_php_type` into the wrapper and box it with the same
  ownership split as the shared callable invoker: owned string/array/object results transfer
  their owner, scalars use the ordinary value boxing, and Mixed results tail-call unchanged.
  The boxing happens before the input box is released in the Mixed-parameter branch.
- Regressions: `test_array_map_untyped_float_callback_boxes_raw_result`,
  `..._materialized_write`, and `..._mixed_return_callback_boxes_raw_result` in
  `tests/codegen/arrays/callbacks.rs`; the float-scale element-write fallback now runs too.

## Float-affine stored maps

- New `FloatAffine { scale, offset }` shape: `$v * F + G`, `G + $v * F`,
  `$v * F - G`, and `G - $v * F` (float or int literals, exact double conversion).
- `__rt_array_map_float_affine` converts each int to double, multiplies, then adds the
  literal offset in source order, and stores a raw `array<float>` (value_type 2). No Mixed
  cells and no promotion branch.
- The raw-float `array_sum()` fast path from the previous block consumes it directly.

## Validation

- `stored_numeric_map`: 36 passed (including float-affine shapes, large values, element-write
  fallback, heap-debug, `--ir-opt` parity, and the now-running float-scale fallback).
- The three generic callback regressions pass.
- `array_map_float_affine_emits_all_targets`: 1 passed.
- `optimizer::`: 874 passed / 0 failed / 1 ignored.
- `arrays::`: 1301 passed / 0 failed / 1 ignored.
- x86_64 release cross + qemu: generic callback fixtures print `9`, `9.5|17`, and
  `2,1.5,1.5`; float-affine prints the full shape/consumer fixture and `39125000`.
- The generic wrapper disassembly shows `bl closure`, `fmov x1, d0`, `mov x0, #2`,
  `bl __rt_mixed_from_value` before returning to the Mixed map helper.

## Benchmarks

- Formal harness (5 runs + 2 warmups): `array_map_float_affine` elephc **1.612 ms** vs PHP
  13.854, PHP+JIT 14.266, C `-O2` 2.322. Neighbouring float-scale 1.586 vs 14.230 / 16.458
  / 1.133; square-affine 1.704 vs 17.048 / 16.056 / 1.003.
- Same-compiler A/B (`array_sum` + stored map, in-process `hrtime`, 5000 maps of 100 ints):
  raw float-affine kernel 0.768 / 0.775 / 0.780 / 0.818 / 0.824 ms; generic
  `runtime.array_map` fallback 18.262 / 18.286 / 18.464 / 18.814 / 19.047 ms. Both printed
  `39125000` (median about 18.46 -> 0.780 ms, about 23.7x). PHP 7.986 ms; PHP+JIT 8.345 ms.
  The parent compiler cannot provide this A/B because it still SIGSEGVs on the untyped
  float-callback fixture; the new compiler's generic fallback is the fair baseline.
