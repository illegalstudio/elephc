# Foreach map fusion via generated iteration local

- Previous attempt inserted `$x = <mapped body($x)>` into the body while `$x` was also the
  foreach value variable. `lower_foreach` typed `$x` as `int` from the packed source, then the
  checked-mul result (int|float) was stored back into that int local; stack-numeric could not
  keep the cell and the transform measured no gain (21.7 vs 20.6 ms) and was reverted.
- This attempt rewrites `foreach (array_map(fn($v) => <arithmetic>, $ints) as $x)` to
  `foreach ($ints as $gen#gen) { $x = <arithmetic($gen#gen)>; <body> }`. `$gen#gen` is an
  unforgeable generated local, so `lower_foreach` declares it as `int`; `$x` remains a normal
  Mixed local and `stack_numeric` proves both it and the accumulator into frame cells.
- The original `$x` keeps every observable behavior: final value after exhaustion, empty-source
  behavior, body writes, `unset`, and by-reference uses. The callback body is pure arithmetic
  with no effects, warnings, or throws (overflow promotes to float), so evaluating it at the top
  of each iteration rather than materializing the whole map is unobservable.
- Fail-closed guards: source must be `array<int>`, not aliased, not mentioned in the body, and
  distinct from the value/key variables; by-reference iteration, typed returns, captures, and
  effectful callbacks keep the generic map. Array writes in the body must target a definitely
  initialized local.
- Tests: `tests/codegen/optimizer/array_map_read_fusion.rs` 24 passed. Covers fusion, keys,
  final value/empty source, body writes, by-reference use, overflow-to-float, initialized array
  push, source write/alias/by-ref/typed rejection, generated-local hiding, and `--ir-opt` parity.
- In-process body timing (1M iterations, map+foreach over 100 ints, 5M element adds): old
  30-54 ms, fused ~2.7-3.1 ms; PHP CLI ~12.1 ms, PHP+JIT ~10.1 ms.
- Native benchmark `benchmark/cases/native/array_map_foreach` (5 runs, 2 warmups, process
  timing, loaded machine): elephc 3.492 ms vs PHP 21.037, PHP+JIT 18.923, C -O2 0.545.
  Neighbouring rows stayed healthy: `array_map_direct_read` 0.833 vs 21.074/17.363/0.356,
  `array_map_mul_reads` 1.619 vs 17.040/20.133/0.431.
- Interleaved A/B against the parent compiler from commit `1f94f6e430` (the parent IR still
  contains one `runtime.array_map`, the fused IR contains none), 1M element adds:
  parent 32.7, 31.8, 94.2, 98.4, 90.9 ms; fused 3.75, 3.32, 12.08, 3.87, 3.04 ms
  (medians roughly 91 -> 3.75 ms, with one load-spike in each series).
- The transform is target-independent and the x86_64 cross build linked and ran under
  `qemu-x86_64 -L /usr/x86_64-linux-gnu`, printing the same `75750000` expected value.
- Suites: `optimizer::` 829 passed / 0 failed / 1 ignored; `arrays::` 1298 passed / 0 failed /
  1 ignored; heap-debug fused-loop test reports a clean leak summary.
