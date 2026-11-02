---
id: gotcha-42-of-the-emitted-assembly-is-stack-spill-traffi-89d5c45d
type: gotcha
title: "42% of the emitted assembly is stack spill traffic, which is what really sets the build time"
description: "str/ldr/mov dominate 61.4M lines; the preload build cannot be made to compile in 2 min without changing that or the reflection ladders"
created: 2026-09-20
sources:
  - path: src/codegen/context.rs
    blob: 7cbf31fd03af9cdf2c13fdb21bfe06f343725a65
    lines: 1-30
    snip: b65713ee6cfb
    anchor: "use std::collections::{HashMap, HashSet};"
superseded_by: "gotcha-codegen-time-is-set-by-repeated-whole-module-sca-c7452e58"
superseded_why: "Its measurement of the assembly stands: spill traffic really is 42% of the emitted lines. Its CONCLUSION does not. A sample of the codegen phase put all string building at 0.4% and one per-body whole-module predicate at two thirds; memoizing that predicate took the build from 255.67s to 142.98s with a byte-identical .s, while six separate volume cuts totalling -31.5% of emitted lines had moved it far less."
---

# 42% of the emitted assembly is stack spill traffic, which is what really sets the build time

## Fact

Breaking the whole emitted `.s` down by instruction (not by symbol, not by body) is what finally
explains a 61.4-million-line Symfony `--web` build:

    9,579,853  15.59%  str
    8,747,054  14.23%  ldr
    7,728,730  12.58%  mov
    5,287,020   8.60%  bl
    4,602,495   7.49%  add
    3,947,148   6.42%  <label>
    3,788,849   6.17%  adrp

`str` + `ldr` + `mov` is **42.4%**, which is the Phase-04 value placement working as documented:
"Phase 04 stores every SSA value in a stack slot and reloads result registers at use sites"
(`src/codegen/context.rs`). It is not a hot emitter anyone can fix; it is the design.

Two things follow, both measured rather than guessed:

* A peephole will not recover it. Only **9,353** reloads in the whole program are a `ldr`
  immediately after a `str` to the same address into the same register — the spill and its reload
  are separated by `adrp`/`movz`/`sub` address computation, so adjacency-based removal finds
  nothing. Real register allocation is the change, not a post-pass.
* The largest bodies are whole-program materializations (see the dispatch-ladder packet), and
  their epilogues pay for it twice: `ReflectionParameter::getDefaultValue` alone carries 14,207
  refcounted cleanup sites, each `movz/movk/sub/ldr/bl decref/bl pcntl_dispatch`, and
  `__rt_pcntl_async_dispatch_preserving` is called 901,163 times across the module (1.5% of all
  lines) because one `pcntl_async_signals()` anywhere turns it on for every body.

**The two objectives are in tension and the numbers say so.** The no-preload build compiles in
1:26 and serves `/` at roughly 19 ms CPU (38.4 ms measured at load 18, and load roughly doubles
it); the container+Twig preload serves `/` at 5.3 ms and compiles in 3:00. There is no preload
setting in between that gets both — the preload is what puts the container and the template in
native code, and that is exactly the code whose volume sets the build time. Closing the gap means
changing value placement or the materialization ladders, not the preload.

## Why

Two sessions have now looked for a hot emitter to fix; the cost is the value-placement design, and the histogram says so in one command
