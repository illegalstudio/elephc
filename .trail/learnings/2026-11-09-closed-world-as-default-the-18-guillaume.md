---
title: "Closed world as default: the 18 suite regressions and their causes"
author: Guillaume
date: 2026-11-09
branch: feat/closed-world-symfony-build-speed
---

## Decided
- A computed-path include in the project's own code (not vendor) links the interpreter, decided early in pipeline.rs like a computed eval; the late check would catch it too but re-runs the whole build (closed_world_tests asserts no "note: the program needs the interpreter").
- resolver/engine.rs hoist_leading_value_include moves every always-run operand left of an include into its own temporary; a bare variable stays in place (php reads a CV after the include, an array element before it).
- builtin_callable_prelude: a literal only used as an array index, an array_key_exists key or a function_exists/is_callable argument declares no wrapper. The throwable trace prelude's $frame['file'] had added a file() wrapper plus 7 stream helpers (~8% of a small program's asm) to every closed-world program.
- machine_peephole small-region gate counts instructions (256), not text bytes: @src line numbers and hashed labels made the same body lose its peepholes in a longer program.
- autoload index file_declares_class checks the namespace: App\ -> ./ made the entry file a "production class" under a wrong name and loaded it twice.
## Tried and ruled out
- A `declared` user-function filter in the wrapper prelude: redundant with the profile-visibility filter (mutation survived), removed.
## Watch out
- Tests that mix a CLI --emit-asm (closed world) with the in-process harness runtime (open world) link two different worlds; fix the world difference, not the test.
- reg52.sh needs an absolute list path (it cd's to the repo root), or it silently runs the whole suite.
- Still open: a literal include in a conditional position fatals in a closed world (bead elephc-lis4).
