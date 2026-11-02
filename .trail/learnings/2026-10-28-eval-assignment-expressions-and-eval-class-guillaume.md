---
title: Eval assignment expressions and eval-class diagnostics need different boundaries
author: Guillaume
date: 2026-10-28
branch: feat/gmp-extension
cost: { minutes: 60 }
touches:
  - crates/elephc-magician/src/parser/expressions/precedence.rs
  - crates/elephc-magician/src/ffi/object_introspection.rs
  - src/codegen/lower_inst/builtins/eval/introspection.rs
  - src/codegen/lower_inst/builtins/types.rs
  - src/script_cache/file_store.rs
---

Two eval gaps needed complementary fixes:

1. Assignment expressions belong in the Pratt chain between `and` and ternary, but statement
   parsing reuses `parse_expr` for write targets (`$this->x += 2`). `parse_assignment` must
   therefore return a non-variable target UNCHANGED without consuming the operator, so
   `parse_property_like_stmt_tail` still sees it. Only `LoadVar` becomes `EvalExpr::Assign`.
   Adding the variant changes the bincode shape: bump `FORMAT_VERSION` and the recorded
   fingerprint together.

2. Compiled helpers cannot see Magician's dynamic class metadata through `get_class()`. The
   eval-aware lowering required an eval-context local, which prelude/AOT helpers do not own.
   A context-free C ABI entry resolves the owning context from the process-local dynamic-object
   registry using the raw object payload as identity, then reuses the existing lookup. Codegen
   tries it only when the eval bridge is linked and falls back to `_class_name_entries` for AOT
   objects. The raw payload must be boxed before `eval_object_class_name_inner`; the fallback
   branch must restore the object pointer and release the temporary stack or `get_class()` on
   an ordinary object segfaults.
