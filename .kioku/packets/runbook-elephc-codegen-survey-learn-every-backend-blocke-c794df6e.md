---
id: runbook-elephc-codegen-survey-learn-every-backend-blocke-c794df6e
type: runbook
title: "ELEPHC_CODEGEN_SURVEY: learn every backend blocker of a --web build in ONE pass instead of one per run"
description: "Codegen is the last phase, so a whole-app build pays its front end before the first refusal; surveying turns five builds into one"
created: 2026-09-18
verified_by: "Used it four times on examples/symfony-app/public/index_preload_twig.php: the first run named three blockers at once where the default build had named one, and the last run reported exit=0"
sources:
  - path: src/codegen/block_emit.rs
    blob: fab72c4d9789b29e6da119863a231572b28f69c4
---

# ELEPHC_CODEGEN_SURVEY: learn every backend blocker of a --web build in ONE pass instead of one per run

## Fact

    ELEPHC_CODEGEN_SURVEY=1 ./target/release/elephc --web examples/symfony-app/public/<entry>.php

`emit_module` keeps emitting after a body fails and returns ONE error listing every function that
did not lower:

    EIR backend error: unsupported EIR backend feature: 3 function(s) did not lower:
      unsupported EIR backend feature: array_reverse preserve_keys for PHP type Mixed (988:20, …)
      unsupported EIR backend feature: IteratorIterator source PHP type Int (2004:44, …)
      unsupported EIR backend feature: runtime_call from PHP type Array(Mixed) to … (361:9, …)

Off by default, and it must stay that way: everything emitted after a failed body is emitted
against half-registered shared state, so a real build still has to stop at the first error. The
survey is a DIAGNOSTIC — read the list, fix the causes, then build normally.

Timings on this machine for `index_preload_twig.php` (169 preloaded files, ~1.8 GB of asm):
front end ~2 min, whole successful build ~7 min. A failing build costs the front end only.

Companion: `emit_dynamic_string_callback_abort` now names the compiled CANDIDATES in its runtime
message, so `array_map callback string does not name a supported callable (compiled candidates:
none)` identifies the call site instead of naming only the builtin.

## Why

A Symfony --web build is minutes of front end per attempt; one blocker per attempt is the expensive way to learn five
