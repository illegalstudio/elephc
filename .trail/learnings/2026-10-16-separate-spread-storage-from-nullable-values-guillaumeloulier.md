---
title: Separate spread storage from nullable values and protect null receiver cleanup
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1836-nullable-array-merges
cost: { minutes: 0, tokens: 0 }
touches:
  - src/ir_lower/expr/indexed_array_literals.rs
  - src/ir_lower/expr/array_access.rs
  - tests/codegen/types/nullable_array_merge.rs
  - tests/codegen/runtime_gc/nullable_array_read_owners.rs
  - docs/internals/the-ir.md
---

## Decided

Preserve implicit-null array-read types for ordinary indexed/associative literal elements and lazy merge consumers. A direct typed container read used as an unpack SOURCE needs a separate storage projection: strip its missing-value alternative only when the remaining shape is a native Array/Hash. The unpack operation still rejects an actual null at runtime. This keeps destination storage aligned with the concrete getter without losing null elements in ordinary literals.

A nullable receiver's null branch must evaluate its offset once and use the shared getter's normal/silent warning flag. An owning boxed null receiver remains parked in the in-flight cleanup chain over offset evaluation and warning delivery. The shared getter consumes the normal owner; the proven-null payload cannot run a user destructor, so detaching that non-owning pin afterwards is safe. The ownership regression belongs to the runtime GC suite and is registered once.

## Tried and ruled out

The published #1859 compiler failed existing regressions for missed-container spread (Array/Hash operand mismatch) and nullable offset warnings. Its null path also skipped offset side effects. A broad attempt to restore physical read typing fixed both CI tests but broke nullable object elements placed in ordinary literals: strict-null checks printed the integer null sentinel. Keep the logical type shared; project only the spread source.

Fixing evaluation and warnings without protecting the null receiver made PHP output correct but leaked 40 Mixed cells / 1760 bytes when the offset threw forty times. Candidate 1 and 2 artifacts/logs remain invalidated. Candidate 3 passes the 31 focused regressions in both IR modes, including the throwing-offset clean-heap check; final CLI/target checks remain separately recorded.

## Watch out

Do not assume a PHP null receiver owns no heap cell. Do not generalize this post-consumption pin detachment to an object/array payload whose destruction can throw: source/receiver pins there have a different lifetime order. Keep ordinary warned-float getter behavior outside lazy merges unchanged. Freeze linked probes and runtime emitters with their compiler before another worktree replaces shared Cargo dependencies; a root rlib alone is insufficient for later relinking.
