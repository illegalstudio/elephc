---
id: gotcha-a-thread-started-by-a-parallel-phase-reads-every-d43446de
type: gotcha
title: "A thread started by a parallel phase reads every compile-wide thread-local as its default"
description: "compile_is_web_sapi() answered false in codegen workers, so eval contexts ran as CLI and Symfony's 404 became a raw dump; compile_thread_context.rs carries them over"
created: 2026-09-27
sources:
  - path: src/compile_thread_context.rs
    blob: 71dbc516100a1a14d136e9f22f3fd8ba98971946
    lines: 1-40
    snip: 4fc1dcdb4e61
    anchor: "use std::collections::HashSet;"
---

# A thread started by a parallel phase reads every compile-wide thread-local as its default

## Fact

The compiler keeps several facts about the compile in progress in `thread_local!`s because they
cannot be threaded through every signature: the PHP profile and SAPI (compilation_context), the
null representation (sentinels), the class/interface/trait declaration order, the linked
extensions, --strict-php, the eager globals, the --web flag and entry script (superglobals, sapi).

A thread started by a parallel phase reads their DEFAULTS, and nothing says so. The first parallel
codegen run built a Symfony binary whose 404 page came back as a raw exception dump: bodies
emitted in a worker lowered the eval context with `__elephc_eval_set_web_sapi(0)` because
`compile_is_web_sapi()` answered the CLI default there. Everything else about the output was
right, which is what made it slow to find (bisected by body index to one method,
App\Kernel::doInitializeContainer, then diffed that body's assembly between the two builds).

src/compile_thread_context.rs captures every compilation-wide thread-local on the compiling thread
and installs it on each codegen worker. A new compilation-wide thread-local must be added there,
or every parallel phase reads its default. Pass-scoped ones (set and restored around one pass:
the lowering guards, the strict-PHP source mode, the parser mode) are at their defaults on the
compiling thread too by the time codegen runs, so they are not captured.

## Why

Any new compilation-wide thread_local! must be captured there, or a parallel phase silently uses its default
