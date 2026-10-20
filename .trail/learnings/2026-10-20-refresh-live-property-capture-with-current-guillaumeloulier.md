---
title: Refresh live property capture with current hash integration and full Linux controls
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1815-property-receiver-reload
---

## Decided
Refresh the existing ready PR #1877 from 80c47f64083450c20e0e76af45586b515d745fc1 onto main 09c634061ae4b68335066fcb56d6764e26a024b8. The only manual rebase resolution is additive GC test wiring: preserve gc_incoming_cache, property_receiver_arguments and unshift_reference_cow once each. Range-diff confirms the implementation is unchanged.

The standalone frozen compiler/library passes 39 focused tests per IR mode. All seventeen property scenarios execute on both Linux architectures, including the callable cases that previously had only local/CI execution evidence: 34 matching PHP outputs with clean heaps. All seventeen assemble for each supported target (85 checks). Direct, first-class and string-alias EIR capture ordering remains checked for all five targets.

The builtin exporter is built with curl support. Export/render/module/comparison, builtin audits, site compatibility and the EIR architecture audit all pass; regeneration has no tracked diff. A local tomllib facade over the already installed tomli allows the Python3.9 host to run these tools without changing repository generators or installing a global runtime.

Keep #1817's implementation separate. A validation-only worktree combines the rebased property fix with #1817 df167cdbce9f070bf94919d652c1bd00b0ca0f84. Its only merge conflict is an explanatory comment beside the shared current-receiver reload. The combined tree passes 110 focused tests in each IR mode, and the exact original #1815 witness returns 3|x,y,3 with clean heaps on macOS and both Linux architectures. Its emitted iOS library functions contain both property-cell capture and the hash merge call.

## Tried and ruled out
An old-head green CI result is not validation of the rebased source. Do not retain the receiver object, re-evaluate it after arguments, or restore a saved property value: those approaches change identity, lose nested mutations or delay destruction. Do not copy the associative feature into the property PR just to make the original witness compile without its dependency.

## Watch out
The standalone manual probe's CLI tests read CARGO_BIN_EXE_elephc at run time; setting it only for rustc produces NotFound even with a valid frozen CLI. Pin it for execution and retain the correctly frozen compiler/workspace dependencies and matching bridge archives.

Trail check's noun matches quote descriptions of rejected object retention, missing callable routes, duplicate decrements and unrelated serializer/Synthetic concerns. The current implementation retains the cell, includes indirect known callables, keeps the consuming helper owner and reload, and registers tests once. The commit/PR explain that reconciliation; old learnings remain immutable.

The PR is ready, so remove stale draft wording from its description without changing isDraft. The original associative acceptance still depends on #1817. Runtime callable parameter property references remain explicitly diagnosed until their separate boxed-reference ABI is implemented. iOS coverage is assembly, not device execution; full CI must be inspected at the new exact pushed head.
