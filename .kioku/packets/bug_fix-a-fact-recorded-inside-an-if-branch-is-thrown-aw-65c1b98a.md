---
id: bug_fix-a-fact-recorded-inside-an-if-branch-is-thrown-aw-65c1b98a
type: bug_fix
title: "A fact recorded inside an if-branch is thrown away with the resolver state clone"
description: "resolve_isolated clones ResolveState per control-structure branch so include effects do not leak; a build FACT recorded there vanishes, which is why a polyfill if-guarded require reported no dropped declarations while the same require at file scope did"
created: 2026-09-22
verified_by: "a four-file fixture bisected one ingredient at a time, cargo test --lib at the 29-failure baseline, Symfony --web 8/8"
sources:
  - path: src/resolver/engine.rs
    blob: be0ef67a85a80659542cdee7bf774ddb2c2a9cc9
    lines: 573-596
    snip: c57c274f73b4
    anchor: "pub(super) fn resolve_isolated("
  - path: src/resolver/state.rs
    blob: adc20b4493150f097a4e3e8504811fda4af653ac
    lines: 37-60
    snip: a6baa8c278c4
    anchor: "pub(super) source_units: super::source_units::SourceUnitCollector,"
---

# A fact recorded inside an if-branch is thrown away with the resolver state clone

## Fact

`resolver::engine::resolve_isolated` CLONES `ResolveState` for every control-structure branch, so
that "include and constant effects do not leak back to the caller". Anything recorded into that
state inside an `if` body is discarded with the clone.

That is fine for an effect and wrong for a FACT. Two fields added for the eager-file claim
decision -- which declarations were dropped, which were bound -- are facts about the build, and a
polyfill writes its delegation as:

    if (\PHP_VERSION_ID >= 80100) { require __DIR__.'/bootstrap81.php'; }

so every drop under it was recorded into a clone and thrown away.

BISECTED on a four-file fixture, one ingredient at a time, in 15-second builds:

    require at file scope          -> lost=true  dropped=["probe_dc"]
    the SAME require inside an if  -> lost=false dropped=[]

and the `if (extension_loaded(...)) { return; }` guards, which were the first two suspects, made no
difference in either direction.

FIX: the two collectors are SHARED across clones (`Arc<Mutex<...>>`), exactly as
`ResolveState::source_units` already is and for the same stated reason. `snapshot()` reads them at
the end of the resolve.

WHY FOUR HYPOTHESES MEASURED AS CONTRADICTORY BEFORE THIS. On the Symfony console the eager trace
reported `dropped=[] bound=0 globals={} nested=["bootstrap81.php"]`: the subtree WAS read, it
contributed nothing, and nothing was reported as dropped. No explanation fits that until you know
the recording itself was being discarded. `extension_loaded` folding, an unexpanded include, a
truncated guarded return and a self-erasing `function_exists` fold were each checked and cleared
first.

AFTER: the console boots far enough that `deepclone_to_array` resolves, and the next wall is the
8 MB default heap (`--heap-size=`). Raising it to 32 MB or 256 MB replaces the clean
`heap memory exhausted` diagnostic with SIGBUS (rc=138) -- a separate fault the message was hiding.

`ELEPHC_EAGER_TRACE=1` prints the claim decision per eager file and is what made this findable.

## Why

It is the fifth console blocker and the reason four earlier hypotheses all measured as contradictory
