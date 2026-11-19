---
title: Codegen harness must mirror the CLI pipeline world
author: Guillaume
date: 2026-11-19
branch: bead-harness
touches:
  - src/pipeline.rs
---

## Decided
- The codegen harness (tests/codegen/support/compiler.rs) must compile the world the CLI compiles: closed world first (Mode::Auto), interpreter-linked retry on bridge reasons, the session surface, the late hash/random/tz gates, builtin_callable_prelude, and one PostTypecheckOptimizer with checker type metadata.

## Tried and ruled out
- Leak failures of Throwable dynamic new are not harness-only: the CLI leaks the same 48 bytes under --heap-debug (bead elephc-4cs6).

## Watch out
- Every CLI build carries the session prelude, whose unserialize() calls open the SPL/DateTime/Reflection checker gates: a harness without it compiles ~16 builtin classes instead of ~91 and misses mixed-receiver crashes.
- Fixture with a literal eval that needs the interpreter: the harness retries with the interpreter linked, as closed_world::rerun_with_interpreter does for the CLI.
