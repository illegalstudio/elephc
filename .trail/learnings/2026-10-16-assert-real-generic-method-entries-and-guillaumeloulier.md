---
title: Assert real generic method entries and match both all-target test suffixes
author: Guillaumeloulier
date: 2026-10-16
branch: review/1822-finalize
cost: { minutes: 0, tokens: 0 }
touches:
  - ".config/nextest.toml"
  - tests/codegen/generics.rs
---

## Decided

Match all-target Nextest tests with `_on_(all_targets|every_target)$` in both profiles, keeping the scoped 180-second budget and the unrelated 60-second default. Folding tests must select instance methods with `method_symbol`, prove their real entries exist, and inspect the entry thunk when sharing is expected.

## Tried and ruled out

The former `touch` assertion searched static symbols for an instance method, so absence passed without checking emitted code. Simply replacing the symbol helper while retaining a transitive-fold expectation fails in both IR modes: the generic static helpers share a body, but their instance callers still name different helper entries. Keep that as a conservative non-folding control; a separate inherited, non-generic static implementation provides identical call targets and a positive thunk assertion. Do not introduce transitive callee canonicalization to satisfy an obsolete test comment.

## Watch out

Two addressable entries do not imply duplicated bodies: one may be a thunk. Conversely, testing that two wrong symbols are not both present proves nothing. Validate both the positive sharing case and distinct-target control on every supported target. Nextest regexes need singular/plural suffix controls; TOML parsing alone does not prove the intended names match. Verification artifacts and scratch crates stay inside Elephc.
