---
title: "Symfony Demo build speed: direct assembly, FxHash, Fable levers (2026-11-09)"
author: Guillaume
date: 2026-11-09
branch: feat/closed-world-symfony-build-speed
---

## Decided
- In-tree assembler reads each slice straight from the mapped .s (prologue + borrowed body + footer, promoted temporaries renamed at name lookup). ELEPHC_ASM_DIRECT=0 keeps the rendered copy. Demo console 3261 G -> 2946 G instructions.
- In a release build without LTO (how freezes are measured), starts_with("//"), strip_suffix(':'), split_once(' ') each call memcmp or build a searcher: hot parsers test bytes / memchr.
- Every src/ file imports crate::fast_hash aliases as HashMap/HashSet; hasher-generic signatures keep std's type. Integration tests build sets via elephc::fast_hash. 2840 G -> 2620 G.
- Fable's levers done: vtable family memo (reachability), instanceof prefilter by parent walk, escaped callable locals from the smaller side, incremental exception-flow fixpoint (ELEPHC_EXCEPTION_FLOW_VERIFY, on in tests), property-name -> classes index for Mixed property sites, __call fallback evaluates args once (was 2^n), method worklist re-walks by name for static properties / callable return sigs / recursive-mixed growth / offset* members. 2620 G -> 2491 G before the checker changes.
## Tried and ruled out
- profbuild2.sh's `pgrep -x elephc | tail -1` can sample ANOTHER session's elephc: filter sample files by `Process: elephc [pid]` (pad/proftop.sh) before reading a profile.
- Converting std maps file by file does not converge (each file breaks its neighbours); convert the whole crate at once. BSD sed has no \b: use [[:<:]] / [[:>:]].
## Watch out
- Lever 3 (one global callable-array lookup body + thin per-scope bodies) is not done: equivalence depends on scope overrides being checked before the public table.
- The lib unit tests have 81 standing failures (same names before and after this work); compare failure lists, not counts.
