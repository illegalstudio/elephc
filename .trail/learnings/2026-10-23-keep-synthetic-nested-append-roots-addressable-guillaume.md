---
title: Keep synthetic nested append roots addressable before scalar write guards
author: Guillaume
date: 2026-10-23
branch: review/deepseek-pr2027-20261023
touches:
  - src/ir_lower/stmt/nested_append.rs
  - src/ir_lower/stmt/nested_plain_writes.rs
  - src/optimize/propagate/stmt.rs
  - src/optimize/propagate/stmt/env.rs
  - src/optimize/tests/propagate/collections.rs
  - tests/codegen/arrays/nested.rs
  - tests/codegen/optimizer.rs
  - ".plans/issue-1985-review-followup.md"
  - tests/codegen/optimizer/nested_scalar_append.rs
---

## Decided
PR2027's marked key/RHS regression exposed a real computed-key scalar append failure, beyond missing test coverage. Keep the reserved synthetic append read attached to the writable variable root during constant propagation, then route integer/float/resource roots through the existing write-context path and scalar Error guard. Preserve ordinary propagation for unrelated reads. The receiver is selected after RHS effects.

## Tried and ruled out
Extending append recognition to scalar locals alone did not help: the AST optimizer had already replaced the internal root variable by IntLiteral42, so the read/push/writeback triple no longer matched. Parsed and propagated AST snapshots isolated that difference. The raw fallback attempted a container operation on an integer and failed backend emission. Preserve the storage root, not a detached scalar read. Do not resolve the main rebase by dropping release_error_string; that cleanup precedes the expression-position Error and an owned-RHS regression verifies it.

## Watch out
Four focused codegen tests pass in both IR modes with exact PHP8.5.10 output and clean heaps; three optimizer structure tests pass. Ten program assemblies cover all five supported targets and both IR modes; Linux/iOS evidence is assembly only. A global statically-scalar RHS-array rewrite remains a separate checker witness; the supported mixed-parameter live-root control passes. The scope is numeric/resource scalar writes, not false-to-array autovivification. Keep all own worktrees and artifacts inside Elephc, preserve other dirty work and external historical trees, and update the existing fork PR branch with a fresh main rebase and exact remote lease. CI must validate the new head.
