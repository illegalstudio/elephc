---
id: runbook-get-a-per-test-head-baseline-before-blaming-your-87a6b0ef
type: runbook
title: "Get a per-test HEAD baseline before blaming your change for a codegen failure"
description: "Exporting the committed tree into a separate target dir gives the failure set at HEAD in about three minutes"
created: 2026-09-20
sources:
  - path: tests/codegen_tests.rs
    blob: 9f1d9dfb4f84d947e183bbd31b66f4a5a1c68693
    lines: 1-40
    snip: 564be388de73
    anchor: "mod managed_pcre2_support;"
---

# Get a per-test HEAD baseline before blaming your change for a codegen failure

## Fact

The suite carries hundreds of standing failures, so a failing test says nothing until you know
whether it failed before you touched anything. Guessing is expensive: in one session two
different changes were blamed for the same seven failures and one of them was reverted, and a
HEAD baseline then showed all seven failing at HEAD as well.

Build the baseline WITHOUT disturbing the working tree:

    B=<scratch>/headbase
    mkdir -p $B && git archive HEAD | tar -x -C $B
    cd $B && CARGO_TARGET_DIR=<scratch>/headbase-target RUST_MIN_STACK=33554432 \
        cargo test --release --no-run --test codegen_tests

`git archive` reads the committed tree only, so nothing in the worktree moves — no stash (which
is shared across worktrees here) and no checkout of files that hold uncommitted work. The
separate target directory keeps it off the tree's cargo lock, so a build can run at the same time.
It took 2m 48s here.

Then run the SAME filter against both binaries and diff the failure NAMES:

    cd $B && ./../headbase-target/release/deps/codegen_tests-<hash> <filters> > baseline.log
    awk '/^failures:$/{f=1;next} f && /^    codegen/{print $1}' baseline.log | sort -u

A test that is new in the working tree simply will not run at HEAD — the run reports fewer tests
than the filter asked for, which is itself the answer for that one.

## Why

Knowing the suite has ~420 standing failures is not enough; you need to know whether THESE tests are among them, and guessing costs a wrongly reverted change.
