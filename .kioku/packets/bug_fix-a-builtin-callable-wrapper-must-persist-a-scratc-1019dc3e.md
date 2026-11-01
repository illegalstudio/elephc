---
id: bug_fix-a-builtin-callable-wrapper-must-persist-a-scratc-1019dc3e
type: bug_fix
title: "A builtin callable wrapper must persist a scratch-backed string return"
description: "array_map with a string builtin callback returned the last element for every element: the wrapper handed back a concat-arena pointer the invoker then rewound"
created: 2026-09-17
sources:
  - path: src/codegen/lower_inst/runtime_wrappers.rs
    blob: c5b3c07ea2747aedfe9afcc99a0c795264736540
  - path: src/codegen/runtime_callable_invoker.rs
    blob: 04d9267e68ed91b69f11ec1f5c0f74aea5680f98
  - path: src/ir_lower/stmt/control_exit.rs
    blob: 775f4fbb5586e4fca28ab8ecc8520859b7dd7761
---

# A builtin callable wrapper must persist a scratch-backed string return

## Fact

A synthetic builtin callable wrapper must PERSIST a scratch-backed `Str` return before the
terminator, because the descriptor invoker rewinds the concat arena the moment it returns.

`array_map('strtoupper', ['ada','lin'])` printed `LIN,LIN`. On unequal lengths the mechanism is
visible: `array_map('strtoupper', ['op-rich','zz'])` printed `ZZ-RICH,ZZ` — every result cell
pointed at the SAME `_concat_buf` address and kept its own length, so a shorter later result
replaced only the prefix. `array_map(fn ($v) => strtoupper($v), …)` was correct on the same input.

Three pieces have to be read together:

1. `__rt_strtoupper` and its family write into the shared concat arena and bump `_concat_off`.
2. `runtime_callable_invoker.rs` saves `_concat_off` before the nested callable call and RESTORES
   it after (`restore_concat_offset_after_nested_call`), then builds the Mixed result cell by hand
   — `__rt_heap_alloc 24`, tag 1, store the pointer/length pair RAW, no persist. That is only sound
   because `emit_boxed_invoker_return` documents the contract: a `Str` return crosses the boundary
   OWNED, and "scratch-backed results are persisted before the terminator".
3. `build_runtime_call_wrapper_function` BUILDS its body rather than lowering it from PHP, so it
   never reached `persist_scratch_return_string` and broke that contract.

The fix asks the question return lowering asks — `string_op_uses_scratch_storage` over the result's
defining op, now re-exported from `src/ir_lower/mod.rs` — so a builtin that already returns
heap-owned bytes is not copied a second time and nothing is orphaned.

Cleared two standing codegen failures:
`codegen::arrays::callbacks::test_array_map_literal_string_callback_over_runtime_array_keeps_result_type`
and `…::test_array_map_dynamic_string_builtin_callback_uses_descriptor_invoker` (arrays module went
from 29 to 27 failures, with no other name changing).

The rule generalises: anything handing a compiled function's `Str` result across a boundary must
either persist it or stay inside the arena scope that produced it.
