# Direct echo keeps the numeric frame accumulator

- `stack_numeric::plan` already proved loop-carried checked numeric producers and their local
  slots, but rejected the final `LoadLocal` when the only observation was a direct `echo $sum`.
  `scalar_observation` admitted a string cast but not `Op::EchoValue`, so `$sum += ...; echo $sum`
  fell back to `__rt_mixed_numeric_add` and a heap Mixed cell per iteration.
- Admitted `EchoValue` and `PrintValue` as stable same-block payload observations. The output
  formatter reads the tag/payload and numeric tags render into concat scratch without adopting
  the cell, so the frame cell remains valid.
- The loop body now uses the inline stack-numeric arithmetic and frame offsets; no
  `__rt_mixed_numeric_add` or per-iteration heap allocation.
- Updated `test_stack_numeric_scalar_rhs_uses_frame_accumulator` (it previously expected the
  runtime numeric helper) and added `test_stack_numeric_direct_echo_keeps_frame_cell` with
  allocation-count bounds. `optimizer::stack_numeric` 23 passed.
- Native benchmark `benchmark/cases/native/int_accumulator` (5M `$n += intval("42")`).
- Interleaved A/B on one machine, 5M `$n += intval("42")`: old 123-610 ms (load-dependent
  outliers; common ~300), new 20-127 ms (common ~43), roughly 7x on the same run.
- Native benchmark `benchmark/cases/native/int_accumulator` (5 runs, 2 warmups, process
  timing): elephc 22.94 ms vs PHP 73.65, PHP+JIT 22.44, C -O2 0.336. `intval_string` stays
  within PHP (92.37 vs 96.55; JIT 71.80, C 44.40) under the same load.
- Full `optimizer::` suite after the observation change: 819 passed, 0 failed, 1 ignored.
  `optimizer::stack_numeric` 24 passed, including direct-echo float continuation after
  `PHP_INT_MAX + 1` in both IR modes.
