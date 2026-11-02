---
id: gotcha-codegen-time-is-set-by-repeated-whole-module-sca-c7452e58
type: gotcha
title: "Codegen time is set by repeated whole-module scans, not by how much assembly is written"
description: "A 50-second sample put string building at 0.4% and one per-body module predicate at two thirds; memoizing it halved the build"
created: 2026-09-20
sources:
  - path: src/codegen/context.rs
    blob: 1a1c4f08af1d846e4858bf818cd951eff147c156
    lines: 145-160
    snip: 70ce4c965243
    anchor: "heap_debug: bool,"
  - path: src/codegen/shared_state.rs
    blob: 6a6fad0c657fbbd3a8730f04a3cf3c05d5b02366
    lines: 60-95
    snip: a91e6719b9e9
    anchor: "label_counter: usize,"
supersedes: "gotcha-42-of-the-emitted-assembly-is-stack-spill-traffi-89d5c45d"
---

# Codegen time is set by repeated whole-module scans, not by how much assembly is written

## Fact

Codegen time on a framework-scale module is set by REPEATED WHOLE-MODULE SCANS in the compiler,
not by how much assembly it writes. Sample it before cutting emitted code.

    zsh scratchpad/profile-codegen.sh 40     # starts a --web build, samples once it is in codegen

What a 50-second sample of the Symfony `--web` codegen phase said, by top of stack:

    Chain<A,B>::try_fold                                22 916
    Map<I,F>::try_fold                                   6 582
    SharedCodegenState::runtime_instance_method_descriptor 3 285
    class_info_by_name                                     729
    core::fmt::write                                       127   <- all string building, 0.4%

So building 42 million lines of text is not the cost. `FunctionContext::new` ran
`module_uses_pcntl_async_signals` and `module_uses_pcntl_signal_handlers` on EVERY body, and each
walks every instruction of every function, method, closure, fiber wrapper, callback wrapper,
extern trampoline and runtime invoker: 745 237 instructions revisited by each of 8 786 bodies,
twice. Memoizing both in `SharedCodegenState`, the way `mixed_string_sharing` and
`count_guard_sharing` already were:

    serial_tail   110.91s -> 37.18s
    codegen       145.75s -> 64.85s
    whole build   255.67s -> 142.98s   (wall 262s -> 154s, user CPU 466.59s -> 137.13s)

and the emitted assembly was byte-identical (1 396 032 370 bytes), which is the proof that
nothing but recomputation was removed.

This supersedes the claim that 42% of the assembly being stack spill traffic is "what really sets
the build time". Spill traffic is indeed 42% of the LINES, and cutting emitted volume by 31.5%
over six separate changes moved the build far less than this one memo did.

The remaining entries in that sample are the same shape and are still there: the shared-codegen
caches are `Vec` linear scans (`runtime_instance_method_descriptor` compares a class name, a
method key, an impl class and a whole `FunctionSig` per entry), and `class_info_by_name` is a
by-name lookup over the class table.

## Why

Six volume cuts totalling -31.5% of emitted lines moved the build less than this one memo did.
