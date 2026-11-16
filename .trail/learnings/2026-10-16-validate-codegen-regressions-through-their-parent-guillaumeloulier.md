---
title: Validate codegen regressions through their parent module registration
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - tests/codegen/types.rs
  - tests/codegen/types/declare_groups.rs
---

## Decided

When adding a codegen test submodule, give each mod declaration its own explicit path and compile/list the real parent module before publication. A standalone focused harness proves the file's tests, but does not prove the normal suite registers that file.

## Tried and ruled out

The declaration-group file passed five isolated tests while its parent accidentally reused the strict_types path attribute. The normal suite silently loaded the strict tests twice and skipped the new regressions. Isolated green results alone did not catch this wiring error.

## Watch out

Check the registered test names and counts from the parent module: five declare_groups and eleven strict_types cases in this change. Retain the isolated repro for fast development, then replay the registered group after the final edit. Do not count a separately included file as proof of CI registration.
