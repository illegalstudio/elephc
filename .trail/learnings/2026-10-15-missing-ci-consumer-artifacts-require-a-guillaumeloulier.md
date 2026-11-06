---
title: Missing CI consumer artifacts require a fresh producer run
author: Guillaumeloulier
date: 2026-10-15
cost: { minutes: 0, tokens: 0 }
touches:
  - src/span.rs
---

## Decided
For a CI consumer that cannot download a named build/native-package artifact, inspect the producer run and its artifact inventory before changing compiler code. PR 1751's failed Linux ARM64 consumers belonged to a run created on October 5, retried on October 15, with no artifacts left. Refresh the complete producer/consumer workflow through the rebased PR rather than retrying only the failed consumers.

The rebased span change remains documentation and one regression test. Compile the actual self-contained src/span.rs with rustc --test to exercise its twelve tests without rebuilding unrelated compiler/runtime code.

## Tried and ruled out
Treating Artifact not found as evidence of a Span::merge regression is unsupported: neither failed shard reached its test executable. Retrying consumers alone cannot recreate absent producer artifacts.

## Watch out
Keep the existing PR and its head branch. Freeze the old remote SHA, rebase on current origin/main, validate the changed surface, squash the update and push with an exact lease. Preserve unrelated dirty worktrees. No full local suite is needed for this test/documentation-only change. Time and token costs were not measured; zero disables historical cost attribution.
