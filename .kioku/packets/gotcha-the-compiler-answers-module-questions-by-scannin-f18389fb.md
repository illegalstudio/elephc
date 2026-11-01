---
id: gotcha-the-compiler-answers-module-questions-by-scannin-f18389fb
type: gotcha
title: "The compiler answers module questions by scanning, and those scans are the build time"
description: "Five scans found by sampling in one session; every fix left the emitted assembly byte-identical"
created: 2026-09-20
sources:
  - path: src/codegen/shared_state.rs
    blob: 117702787403d45c8d061e2942457d191819c429
    lines: 40-120
    snip: 2166cda3b5ec
    anchor: "runtime_instance_method_descriptors:"
  - path: src/optimize/reachability/graph.rs
    blob: a49bc9a95eca0a29e9eacefe6b1da0addaa0f1fd
    lines: 420-460
    snip: 336510224f68
    anchor: "fn seed_live_methods(&mut self) {"
---

# The compiler answers module questions by scanning, and those scans are the build time

## Fact

The compiler answers questions about the module by SCANNING it, and the scans are what the
build time is made of. Five instances were found by sampling in one session; each fix left the
emitted assembly byte-identical, which is how you know only recomputation was removed.

| what | how it was asked | fix |
|---|---|---|
| `module_uses_pcntl_async_signals` / `_signal_handlers` | every instruction of every body, ONCE PER BODY, from `FunctionContext::new` | memoized in `SharedCodegenState` |
| `superglobals::uses_shared_ref_cell` | every instruction of every body, per name, per eval scope entry | one walk collecting the whole set |
| `SharedCodegenState::runtime_instance_method_descriptor` | linear `Vec` scan comparing three strings AND a `FunctionSig` per entry | bucketed by a hash of the name triple |
| `class_info_by_name` / `interface_info_by_name` | linear scan of every class, allocating a lowercased key for EVERY entry | index by PHP key, built once behind a `RefCell` |
| `GraphState::seed_live_methods` | cloned every live class name per fixed-point round, plus three `String` clones per (class, method) pair | `mem::take` the set, reuse one probe buffer |

Measured effect of the first one alone, on the Symfony `--web` build:

    serial_tail 110.91s -> 37.18s
    codegen     145.75s -> 64.85s
    whole build 255.67s -> 142.98s   (user CPU 466.59s -> 137.13s)

Across all five, codegen went 145.75s -> 51.24s and the declaration-pruning phase 22.52s ->
17.07s. For comparison, six changes that cut the EMITTED ASSEMBLY by 31.5% moved the build far
less.

How to find the next one:

    zsh scratchpad/profile-phase.sh <start-delay-seconds> <sample-seconds>

Pick the delay from the last `--timings` breakdown; the table only prints at the end, so there is
no marker to wait for, and a loaded machine stretches every phase (choose the window from a run
at a similar load). Read "Sort by top of stack": an iterator adapter (`Chain::try_fold`,
`Map::try_fold`) near the top always means a scan, and a large `libsystem_malloc` share means a
lookup is allocating per candidate rather than per answer.

Still open at the top of the codegen sample: `resolve_reflection_class` (9%), a `SipHash`
`Hasher::write` at 7% (some hot map still uses the std hasher where `crate::fast_hash::FastMap`
is the house type), and in EIR lowering `class_method_signature`,
`is_same_or_descendant_class`, `class_implements_interface_for_ir` — all by-name hierarchy
queries of the same shape.

## Why

Six changes cutting 31.5% of the emitted assembly moved the build less than one of these did.
