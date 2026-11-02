---
id: gotcha-another-claude-session-s-test-suite-makes-every--3ec279ad
type: gotcha
title: "Another Claude session's test suite makes every timing on this machine fiction"
description: "Load 30 from a parallel worktree turned a 180s build into 296s and a 6ms request into 10ms; check the load before trusting any number"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: d56f30aca7e6c6467e39f3d7c9fa3e1344fc46b8
    lines: 76-90
    snip: ebea0e32fc13
    anchor: "pub(super) fn emit_module("
---

# Another Claude session's test suite makes every timing on this machine fiction

## Fact

Timing on this machine is only meaningful when nothing else is running, and other worktrees of
this same repo run their own multi-hour suites. Measured on 2026-09-20: a Symfony `--web` build
that takes 180 s on an idle machine took 296 s while another session's `codegen_tests` ran at
568% CPU alongside Kaspersky at 350%, load average 30.

The tell is a phase that CANNOT have been affected by the change under test. A label-formatting
change was briefly blamed for the 296 s until the table showed "Checking types" had gone 12.9 s
-> 39.1 s as well, and type checking runs long before any label is emitted. Every phase had
scaled by roughly the same factor.

    ps -eo pid,%cpu,rss,comm | sort -k2 -rn | head    # look for another worktree's test binary
    uptime                                            # load average

The same trap applies to request latency: the identical Symfony binary read 12.5 ms, then
9.9-10.5 ms, then 5.3 ms CPU per request as background work drained away. CPU-per-request is
load-INDEPENDENT in principle and is not in practice, because cache and memory-bandwidth
contention inflate it about 2x.

Before believing any before/after timing: check the load, kill stray `index`/`php -S` servers
from earlier runs, and prefer a load-independent quantity when one exists -- emitted assembly
BYTES and line/label counts settled the label change when the clock could not.

## Why

Two builds and several latency readings were misattributed to code changes before the load was noticed
