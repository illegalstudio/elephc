# Issue1494 review follow-up

- [x] Refresh both unresolved review threads on the existing PR1568 branch.
- [x] Rebase the five feature commits onto origin/main ecc48af083.
- [x] Preserve a byte-for-byte backup of all local review changes and the seven strict native fixtures.
- [x] Integrate PR1873 reference-key leases and PR1874 hash destructor ordering without weakening expectations.
- [x] Resolve the test registration conflict by keeping both modules.
- [x] Pass all seven builtin documentation gates with no generated drift.
- [x] Build and freeze the final default compiler, all four eval bridge archives, native probe and runtime emitter.
- [x] Pass 44 focused interpreter unit tests.
- [x] Pass 44 interpreter units and 49 focused tests per IR mode (98 executions).
- [x] Pass all28 opaque CLI cases at PHP82/85 and22 profile controls (all7 profiles in AOT).
- [x] Pass70 combined PR1867 native profile controls, including PHP82 copied-empty-array writes.
- [x] Pass40 combined target object pairs and24 macOS/Linux executions.
- [x] Reconcile Trail findings and prepare one commit for the existing fork branch.

Publication uses a fresh origin/main rebase and the exact remote lease recorded below.

Review fixes evaluate implicit element values before saturated-key insertion checks,
allow the final free PHP_INT_MAX key (including multi-value array_push), preserve the
successfully inserted prefix on overflow, and release temporary insertion/copy owners.

The original remote head is4b3c95df8b22308ee68b4e7fed69cee9d5aeefbc; publication must use
that exact lease on fork/fix/1494-negative-key-append-profile. Never publish a replacement PR.

The strict seven-case test leaf remains byte-identical to the saved review work.
Reference-key cleanup and destructor insertion order are independent main defects with
their own PRs1873 and1874. They are included here so the original regressions can prove
complete PHP output and clean heaps, instead of hiding either defect.

All evidence stays in .claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/pr1568/.
Candidate1,2,3 invalid/partial evidence remains immutable; candidate4 holds the combined source.
The existing iOS Magician build failure is separately pinned to unchanged elephc-pcntl OS
cfg definitions. Linux checks and native macOS proof do not establish iOS eval support.

## Refresh on 2026-10-20

Rebased onto `487cae6abc7d98c1f16bb99042e491bb8a32e27b`. Retain main's native
spread metadata support and share saturated cursor/occupancy validation with
ordinary eval entries. Tests cover the final free key and five normalized spread
shapes, without modifying the original seven maximum-key fixtures.

Current evidence lives under
`.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/pr1568-review-20261020/candidate1/`.
It includes 136 focused interpreter units, 80 original/reference/resource CLI
executions, 22 profile controls, 24 additional issue1647 executions, 40 program/
runtime assembly pairs and 24 native target executions. Output and clean-heap
assertions remain exact. Both Linux Magician checks pass; both iOS checks hit the
same unchanged pcntl OS-cfg baseline before reaching this change.

Public native default arrays with spreads have an independent materialization
failure, preserved with complete PHP82/85 witnesses in the dedicated internal
`fix/native-default-spread-materialization` worktree. The normalized-constructor
coverage here does not claim that native callable/default pipeline fixed.
The copied-array PR1867 integration checkpoint remains staged and untouched;
its older conditional evidence is not presented as a new current-base replay.

The next publication must recheck the current remote head, expected
`8ecbb8ce3ac67250ba09c0edf2896e232a1b08b1`, on the existing fork branch
`fix/1494-negative-key-append-profile`. Keep one feature commit.
