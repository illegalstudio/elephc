---
title: Array write computed keys precede RHS and stable receivers follow it
author: Guillaumeloulier
date: 2026-10-14
branch: fix/1653-review-computed-keys
cost: { minutes: 0, tokens: 0 }
touches:
  - src/parser/stmt/assign/postfix.rs
  - src/parser/expr/assignment_targets.rs
  - src/parser/expr/pratt.rs
  - src/optimize/propagate/stmt.rs
  - src/ir_lower/stmt/nested_array_writes.rs
  - src/ir_lower/expr/call_operand_owners.rs
  - src/ir_lower/stmt/array_write_storage.rs
  - src/ir_lower/stmt/array_write_core.rs
  - src/ir_lower/stmt/property_array_writes.rs
  - src/ir_lower/stmt/static_property_writes.rs
  - tests/codegen/arrays/computed_write_order.rs
---

## Decided
For issue #1653 / PR #1666, capture computed compound-assignment dimensions before the RHS, but read a bare PHP variable dimension after the RHS. Stabilize effectful keys once before settling the RHS; apply the same distinction to statement, expression, instance-property and static-property forms.

For a nested write with a local, simple property or static-property root, capture computed keys left to right, evaluate the RHS, then fetch/autovivify the parent. Leave bare variable keys in the prepared place so they are loaded at store time. Preserve this operand distinction through AST constant propagation.

Use the existing call-operand ownership stack to pin fresh or borrowed computed keys across RHS replacement, later key evaluation and exceptions. A named wrapper exposes the hidden local without inventing a second ownership mechanism. Retire keys in reverse order after the write.

If a concrete integer-array store casts an owning Mixed arithmetic result to an integer, release the original box after the store has been published. The scalar cast does not consume that box. Local, property and static-property stores all use this rule.

## Tried and ruled out
Deferring an effectful whole target changes key/RHS order. Capturing every key early changes PHP bare-variable behavior. Fetching a parent before a RHS that only observes it still exposes premature autovivification. Checking only value/root reassignment syntactically does not cover that observer.

## Watch out
Keep effectful receiver-call roots on their existing path; this change only defers roots that can be fetched without replaying a base expression. Test complete JSON values and heap cleanliness, including an earlier allocated key when a later key or the RHS throws. Pass runtime lengths explicitly into fixtures rather than relying on a function-only GLOBALS argc read.

Reviewed head: 3a2ca069e6ca29737b838f067dbd7a7278f541c9. Rebased onto 62cfeb85efb1d7bbcd149c528a8bc97d959fdbde; all three original patches are unchanged by range-diff. Time and token costs were not measured; metadata is explicitly zero rather than inferred across unrelated worktrees.
