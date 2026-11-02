---
title: Retain missing array-read provenance through AST lazy pruning
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1836-nullable-array-merges
cost: { minutes: 0, tokens: 0 }
touches:
  - src/optimize/control/switch.rs
  - src/optimize/control/prune/expr.rs
  - src/ir_lower/expr/lazy_branches.rs
  - tests/codegen/optimizer/array_read_match.rs
---

## Decided
Keep selected array-read arms/defaults inside their lazy merge until EIR chooses nullable storage. Constant match pruning may discard unreachable arms but must retain a single selected read arm or a default-only match. The same guard applies when pruning constant ternaries, short ternaries and null coalescing, including reads wrapped by error suppression.

A retained short ternary whose left value is PHP null must branch on false explicitly: that value uses the raw I64 null sentinel, while LoweringContext::truthy forwards ordinary I64 conditions unchanged. Receiver/key evaluation, warnings and payload ownership remain observable.

## Tried and ruled out
The first 19 dynamic function fixtures all passed while the exact top-level issue reproducer still crashed: AST pruning had already replaced match(true) with its raw read, bypassing the new EIR merge path. After preserving that context, a warning-count check exposed a raw-null short ternary taking its value branch and skipping the default lookup. Checking stdout and clean heap alone missed that branch error.

## Watch out
Replay the exact original program as well as parameterized variants. Count warnings separately from value and heap checks. Scalar-only selected values still fold normally. The final focused set has 24 regressions passing with EIR optimization on and off; cross-target compile/runtime gates remain separate evidence.
