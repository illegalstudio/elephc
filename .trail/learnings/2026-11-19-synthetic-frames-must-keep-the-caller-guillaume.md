---
title: Synthetic frames must keep the caller line; a TaggedScalar is never a box type
author: Guillaume
date: 2026-11-19
branch: cg3-objects
touches:
  - src/ir_lower/context.rs
---

## Decided
- A synthetic (hidden) activation in a module that can observe a backtrace publishes through `__rt_php_frame_publish_bt`, so `[32]` caller line and `[40]` caller file survive; its reader stays null so backtraces omit it. `DiagnosticCallerLine` and `DiagnosticCallerFile` both read the nearest source frame.
- `box_value_as_mixed(value, ty)` takes the BOX's result type; a `TaggedScalar` there is normalised to `Mixed`, otherwise the next boxing wraps the cell again and `??=` sees a non-null box around null.
- `fold_attr_value` records the expression's own line (`expr.span.line`), not the attribute's.

## Tried and ruled out
- Reading the descriptor-reference failures as an IR difference: caller and callee IR are identical to main; the gap is in codegen/runtime (bd elephc-h17m.57).

## Watch out
- Tests in files main added (descriptor_reference_layouts, nullable_ref_writeback, datetime_lazy_evaluation) did not exist before the merge, so "passed before the merge" is no evidence the branch handled them.
