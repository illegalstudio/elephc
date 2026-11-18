---
title: Loop-carried read before first store keeps definedness flags
author: Guillaume
date: 2026-11-18
branch: bead-php-parity
cost: { minutes: 565, tokens: 6343347 }
touches:
  - src/ir_lower/expr/mod.rs
---

## Decided
A read of a local before the first store a loop reaches further down its body (isset($d), $d ?? x, plain $d) is a definedness question, not a constant NULL. main added a branch in lower_expr (Variable) that answers lower_undefined_variable_read for a slot-less name, which pre-empted the branch definedness flags; loop_carried_names (local_definedness.rs) routes such names through lower_variable_read, and lower_guarded_read reserves a Mixed slot for the guarded load.

## Tried and ruled out
Declaring the slot up front in begin_body: every store of the name would be Mixed even when the flag is never needed.

## Watch out
The slot is reserved at the read, so a later int store into it relies on store_local boxing into a Mixed slot.
