---
title: Eval by-ref method result used by value must copy and release the pending reference
author: Guillaume
date: 2026-11-20
branch: bead-eval-byref
cost: { minutes: 126, tokens: 441872 }
touches:
  - crates/elephc-magician/src/interpreter/expressions.rs
  - crates/elephc-magician/src/interpreter/expressions/null_coalesce_assign.rs
  - crates/elephc-magician/src/interpreter/statements/dispatch.rs
  - crates/elephc-magician/src/context/runtime_state.rs
---

## Decided
eval_by_ref_return leaves a pending reference (target + retained value, and for `return $this->p` a retained owner object) that only a reference binder takes. By-value callers (eval_owned_expr, expression statements) now take it, release both retains through eval_release_pending_return, and eval_owned_expr detaches the result like a property read. set_pending_return_reference now returns the whole unclaimed pair so a replaced one also frees the owner.
## Tried and ruled out
Only adding call results to eval_owned_result_may_share_storage fixed the aliasing but still leaked 10 to 23 blocks: the owner-object retain inside the ObjectProperty target was never released.
## Watch out
Any new consumer of a call result in eval must either bind the pending reference or release it; by-ref method as value, as statement and via a function must stay leak clean under --heap-debug.
