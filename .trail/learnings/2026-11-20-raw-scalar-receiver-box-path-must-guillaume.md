---
title: Raw scalar receiver box path must keep the Mixed/Union receivers
author: Guillaume
date: 2026-11-20
branch: bead-arr-reg
cost: { minutes: 120, tokens: 425955 }
touches:
  - src/ir_lower/stmt/array_write_core.rs
---

## Decided
`lower_array_assign_from_value` sends a local to the boxed array writer when it loads as Mixed, Union, or (since d1f15a96ac) a raw I64/F64. d1f15a96ac replaced the Mixed|Union condition with I64|F64 instead of adding to it, so a `mixed` local took the general path: a float key was diagnosed twice (`$rows[1.9] += 5` printed two Deprecated lines) and the array local was freed twice ("heap debug detected bad refcount" for `$context['_parent'] = $context`). The condition is now Mixed | Union | I64 | F64. The two existing tests (float_key_compound_receivers, generic_array_contract) cover it.

## Tried and ruled out
The owner-transfer peephole (497670a27e) and array_to_hash_steal: both probes fail with `--ir-opt=off`, so not an optimizer pass.

## Watch out
When a fix widens a `matches!` on a representation, add the new case; do not swap it. Bisect cheaply with `--ir-opt=off` first: it clears or implicates the IR passes in one compile.
