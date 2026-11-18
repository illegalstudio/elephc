---
title: Nullable object parameter must retire an array-element argument box
author: Guillaume
date: 2026-11-18
branch: cg2-objects
cost: { minutes: 514, tokens: 6069236 }
touches:
  - src/ir_lower/context.rs
  - src/types/checker/functions/returns.rs
---

## Decided
A bare `object` return accepts a Mixed value at check time (php checks the returned value; named classes use the nominal guard). The forwarding nominal guard of a `?C` parameter forwards the SAME owned Mixed cell, so `owning_box_behind_borrowed_nominal_guard` must name every producer of that cell, array-element reads (`Op::RuntimeCall`) included.

## Tried and ruled out
Marking the guard result `Owned` (emit_owned_value) did not help: the call-cleanup asks `value_is_owning_temporary` of the argument, and `value_is_borrowed_gradual_nominal_guard` answers false for any forwarding nominal guard by design (parent-constructor forwarding relies on it).

## Watch out
A leak check needs `--heap-debug`; `$l[$i]->m($r[$i])` against `?C` leaked 1 box + 1 object per call, the plain `C` parameter did not.
