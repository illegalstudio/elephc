---
id: gotcha-the-codegen-suite-has-a-large-standing-failure-c-90dc84ce
type: gotcha
title: "The codegen suite has a large standing failure count; judge a change against it, not zero"
description: "How many codegen_tests failures are expected on a clean tree, and how to tell a regression from the baseline"
tags: [testing, baseline]
created: 2026-09-16
verified_by: "cargo test --release --test codegen_tests, 2026-09-16, 4538 s"
stale_when: "the suite's failure count is deliberately reduced"
sources:
  - path: tests/codegen_tests.rs
    blob: 9f1d9dfb4f84d947e183bbd31b66f4a5a1c68693
---

# The codegen suite has a large standing failure count; judge a change against it, not zero

## Fact

`cargo test --release --test codegen_tests` does NOT pass on a clean tree. Measured on the
Symfony `--web` branch:

    2026-09-15  9677 passed  423 failed
    2026-09-16  9691 passed  418 failed   (after six compiler fixes + seven new tests)

So a run that ends `FAILED` says nothing on its own. Compare the FAILURE COUNT and, when it moves,
the failing test NAMES against the previous run -- a drop or a flat count with the same names is
clean; a rise, or the same count with different names, is the signal.

Two traps:
- The test binary is compiled when the run STARTS. A source change made while the suite is
  running is not in it, and the result does not cover that change. Re-run.
- Running two suites at once on this machine (another worktree, another session) makes individual
  tests fail on resource contention. `codegen::eval::*` failures that pass in isolation are the
  usual symptom.

The run takes ~75 minutes, so launch it detached (`nohup ... & disown`) rather than from a tool
call that can time out and be restarted from zero.
