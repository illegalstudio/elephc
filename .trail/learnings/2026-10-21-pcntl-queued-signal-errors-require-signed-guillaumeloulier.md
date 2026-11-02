---
title: PCNTL queued signal errors require signed C int width
author: Guillaumeloulier
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - src/codegen_support/runtime/system/pcntl.rs
  - crates/elephc-pcntl/src/signals.rs
---

## Decided

The PCNTL pending-signal dispatcher must classify the signed C int result of elephc_pcntl_signal_next at 32-bit width: cmp w0 on ARM64 and test eax on x86_64. The callback returns minus one on queue read failure and leaves the output record untouched. It is unsafe to read siginfo unless that result indicates success.

## Tried and ruled out

The 64-bit comparisons treated a zero-extended minus one as positive. A compiled console cache-clearing operation consequently indexed its handler table with an uninitialized stack address. A native regression linking the emitted dispatcher to an actual C callback reproduced a fatal signal before the fix; after it, error, empty queue, one record and reentrant dispatch all pass. The test also checks mask restoration and guard state. Eight focused PCNTL unit neighbors pass.

## Watch out

Sources: src/codegen_support/runtime/system/pcntl.rs and its tests.rs submodule; the C contract lives in crates/elephc-pcntl/src/signals.rs. Target-emission checks cover five targets; this host executed only macOS ARM64, with native Linux execution still for a Linux runner. This fixes the unsafe error boundary, not the as-yet-unexplained underlying queue read error during cache clearing. Full application recompilation and execution must follow before claiming that command fixed.
