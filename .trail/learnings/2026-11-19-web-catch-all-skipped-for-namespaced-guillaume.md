---
title: "--web catch-all skipped for namespaced and PDO programs"
author: Guillaume
date: 2026-11-19
branch: feat/apm-v2
touches:
  - tests/web_tests.rs
---

## Decided
- `web_prelude::inject_if_web` cannot wrap a program that holds a namespace (hoisting declarations out of the try across namespace blocks breaks their scope), so it leaves such programs unwrapped. `web_prelude::wrap_resolved_catch_all`, called in `pipeline.rs` after autoload (the resolver has flattened every namespace by then), wraps any plain `--web` program that still has no catch-all. Same idea as `application::wrap_bootstrap`.
- The catch-all is recognised by its `elephc_web_handle_uncaught_exception` call (`web_prelude::answers_uncaught`), not by its variable name; the APM prelude uses the same detector.

## Tried and ruled out
- Suspecting PDO itself, the throw path or `_exc_handler_top`: the assembly showed `main` had no "push EIR exception handler" at all. The PDO prelude simply brings namespaced `Pdo\…` classes, which made the program "namespaced".

## Watch out
- Symptom: worker exits 255 with no message (the Uncaught report goes to stdout, the response buffer), client sees a closed connection instead of a 500.
- Any framework app (namespaced) was affected too; regression tests: `web_uncaught_exception_in_a_namespaced_program_returns_500`, `web_uncaught_exception_with_pdo_returns_500` in tests/web_tests.rs.
