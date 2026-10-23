---
title: Refresh tagged scalar truthiness without losing nullable pointer handling
author: Guillaume
date: 2026-10-23
branch: review/deepseek-pr2000-20261023
touches:
  - ".plans/issue-1996-review-refresh.md"
---

## Decided
Refresh PR2000 on current main while retaining both representation fixes: nullable scalar unions dispatch through TaggedScalar storage, while nullable pointer unions stay on the raw-address arm for truthiness and empty. Preserve main's null-assignment and exact-boolean sort regressions together with the PR's nullable scalar regression.

## Tried and ruled out
Taking either side of the rebase conflict alone would lose the other representation's safety rule or drop existing tests. The old red macOS CLI shard never ran a test: it failed downloading nextest-archive-macos-aarch64. Its run retained134 artifacts but no nextest build archives, so a failed-job-only rerun cannot restore the producer. Refresh the branch and run a complete workflow with ci:macos already present before plan executes.

## Watch out
Forty focused tagged/null/pointer/sort codegen tests pass with IR optimization enabled; six direct truthiness/empty/null/sort/pointer controls pass with IR disabled. Ten program assemblies cover all supported targets and both modes, with Linux/iOS local evidence assembly-only. Keep the existing fork PR and exact remote lease, no upstream push or new PR. ci:macos is a preserved manual CI tier label; it is not a managed target label. A green PR tier alone does not prove the skipped merge-queue tiers.
