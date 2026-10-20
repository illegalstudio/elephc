---
title: Capture live array_unshift property cells across arguments and callable aliases
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1815-property-receiver-reload
touches:
  - docs/internals/the-ir.md
  - examples/mutating_property_arguments/.gitignore
  - examples/mutating_property_arguments/main.php
  - src/builtins/array/array_unshift.rs
  - src/codegen/lower_inst/arrays.rs
  - src/codegen/lower_inst/builtins/arrays/boxed_unshift.rs
  - src/codegen/lower_inst/builtins/arrays/unshift.rs
  - src/ir_lower/expr/callable_resolution.rs
  - src/ir_lower/expr/ref_place_args.rs
  - src/ir_lower/expr/ref_place_args/live_property.rs
---

## Decided

Capture array_unshift instance properties through their managed reference cell at the receiver argument's source position. Publish the hidden cell owner before binding, let temporary/rebound objects die at PHP's normal point, and reload the cell payload before COW. Detach the in-flight owner before retiring its cleared lease. Static properties use their process-lifetime storage. Packed Mixed values use the canonical boxer and transfer one box owner into the new array slot.

Known first-class builtin calls must enter the same capture path as direct calls. Prepare property storage in both ClosureCall and ExprCall effect traversal; string callable names may only resolve during lowering. Potential reference storage never selects a passing mode: by-value arguments still snapshot, and readonly value arguments remain legal. Keep callable reference policy in effects/callable_properties.rs.

The independent consuming reference-cell COW repair is PR #1876. The original associative #1815 witness still needs PR #1817's Mixed-hash helpers; validate their integration without copying that feature into this fix.

## Tried and ruled out

Retaining the receiver object delays its destructor. Re-evaluating the receiver targets a replacement object. Writing a saved property value back loses argument mutations. A clean heap alone misses stale values and premature payload destruction.

Relaxing callable lvalue validation alone cannot enable native properties through runtime callable parameters: the boxed-reference storage guard still rejects that representation. Keep this preexisting limitation diagnosed until a separate ABI extension is implemented.

## Watch out

Retain the earlier COW copy, clone before/during/after capture, argument exceptions, throwing owner destructors, fresh object/string argument payloads, and receiver self-cycles as independent controls. Check complete output as well as heap cleanup with IR optimization on and off. Exercise both Linux architectures; iOS assembly is compile evidence only. All-target EIR tests must retain a real function body and cover direct, first-class and string-alias calls.

Trail check's noun-triplet matches in the docs/tests/boxer are lexical overlap with old failure descriptions and positive validation prose. This change keeps the COW helper owner, current-slot reload, and unwind lease; it does not change serializer projection or Synthetic-root inference. Its callable-path warning did expose a missing route, now covered by the same capture helper and regression tests.
