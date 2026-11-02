---
title: "Single-core build time: what cut 46% of a Symfony build"
author: Guillaume
date: 2026-10-29
branch: reconcile/dirname-symfony
---

## Decided
- Build time is judged on ONE core, by total CPU, and when the machine is loaded by
  `instructions retired` from `/usr/bin/time -l`. On this Mac, CPU time is not load-independent:
  it is inflated by other sessions' test suites (load 40-80).
- Symfony `--web --closed-world`, single core: 1,915 G → 1,029 G instructions (−46%). Quiet
  machine: 120 → 66 s CPU, 128 → 78 s wall. All 8 routes stay identical to php.
- Every whole-program answer that codegen asked per call site now lives in `Module`'s
  `FactCache`, tagged with the module shape:
  - variant groups;
  - interface activation events;
  - extern keys;
  - `classes_by_method`;
  - `strict_descendants`;
  - `name_addressable_hash_classes`.
- Other fixes, by phase:
  - Autoload: `ReferenceSet::owning` moves the class-string defaults instead of cloning them per
    statement; `declared_keys` grows incrementally.
  - Lowering: effect refinement uses a once-built `ClassIndex`; `reflection_family` bounds the
    scan for `mixed` receivers.
  - Checker: `deferral::unknown_parent_refusals` defers a class with an unprovided parent before
    the first full check (cold deferral memo only); folded function-name index.
  - Prune: `applied_hazards` (the global hazard expansion ran after every scanned body);
    argument normalization only for the names that read it.
  - Codegen:
    - runtime call wrappers share one module copy (`take_wrapper_module`) instead of cloning the
      module per wrapper;
    - callable-array instance targets come from a once-built base with `Arc` signatures.

## Tried and ruled out
- Review claims that measurement disproved:
  - the checker's class-table clone/compare costs only 0.5 s;
  - `expr_invalidation` allocation was not the hot `Vec::from_iter` (it was prune's argument
    normalization).
- Caching string descriptor cases by arity: no gain. The cost under them is real wrapper
  emission.
- Recording branch candidates in `aarch64_relax`'s first pass: −0.6% instructions for +700 MB
  peak. Reverted.

## Watch out
- The deferral memo (`ELEPHC_DEFERRAL_MEMO`) already avoided the second full check on warm
  builds, so the precheck only pays on cold ones.
- Peak memory rose 2.87 → ~3.2 GB, mostly the long-lived wrapper module copy.
- Next levers:
  - The relax/split re-scans of the 1 GB `.s`: have the emitter record labels and branches, then
    emit objects directly to drop `as` (~12 s).
  - A worklist for the checker's passes 3-4, which re-check every body for 1 moved signature.
- Tools: `elephc-scratch/tools/profbuild.py`, `sampleagg.py`, `samplechildren.py`,
  `failcmp.sh`, `libcmp.sh`, `llm_review.py`. Reviews are in `elephc-scratch/buildreview/`.
