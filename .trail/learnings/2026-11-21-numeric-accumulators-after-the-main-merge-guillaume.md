---
title: "Numeric accumulators after the main merge: bitwise typing and builtin argument shapes"
author: Guillaume
date: 2026-11-21
branch: bead-numeric-shapes
touches:
  - src/types/checker/inference/ops.rs
  - src/ir_lower/expr/call_arg_coercion.rs
  - src/ir_lower/context.rs
---

Cause 1: the branch's string-aware bitwise typing (str & str is a string) made every bitwise op with one boxed operand Mixed, so $mask = ($argc + 8) & 1023 and $n ^= array_sum(...) boxed their accumulators. origin/main typed all bitwise ops int. Rule now: Mixed only when BOTH operands can be strings (ops.rs may_be_string_operand). Also numeric_binary.rs value_is_numeric_only must list every fused numeric runtime fn (ArrayMapSumAdd/Safe). Cause 2: the branch's boundary coercions in call_arg_coercion.rs (gradual Mixed to float cast, array<int> to array<mixed>) also hit registry builtins (declared_params false), hiding sqrt($i + c) from the range reducer and adding an array_to_mixed per call that blocks LICM; ctx.builtin_arg_sig (set around lower_builtin_call_args, compared by signature address) skips them for builtin arguments only, never for undeclared user params. Cause 3: store_local emitted release_local_slot twice in a loop (second block is a subset of the first), which made array_sum_int_direct reject its scaffold (releases > 2). Cause 4: loop_storage checked_scalar_arithmetic_type must accept nested checked +,-,* and a float accumulator fed by a boxed operand, and an assignment-expression operand. Left open: concat_reset sum_squares (acc stays Mixed because of a fresh $sq = $i*$i temporary; typing it int|float in the loop env did not unbox acc), stack_numeric x3, min_max overflow allocations, buffer_access mixed-bool call may_warn shape, leaf_runtime diagnostic line; release_local_slot x2 and integer_destinations emitters fail only because retired prelude functions stay in the module (callable-bloat bead).
