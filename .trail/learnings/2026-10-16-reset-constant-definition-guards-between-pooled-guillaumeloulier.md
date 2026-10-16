---
title: Reset constant-definition guards between pooled web requests
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen_support/data_section.rs
  - src/codegen/lower_inst/builtins/scalar_metadata.rs
  - src/codegen/web.rs
  - tests/web_tests.rs
---

## Decided

Register static constant-definition guards as typed request-local words in DataSection. The web reset consumes this metadata after cleanup callbacks and before arena reset. One guard remains shared by const and literal define within a request.

## Tried and ruled out

A real one-worker pool regression failed on request two: stale BSS guards rejected definitions from a fresh PHP request. Resetting only constant value storage is insufficient. Do not infer reset ownership from symbol prefixes or disguise a guard as a heap-owning function static.

## Watch out

Emit reset after all functions and the handler body have registered their guards. Clear guards after destructors, handler releases and resource callbacks, because those callbacks can define constants. Assert repeated requests use the same PID; process isolation can hide this bug. The normal CLI duplicate-definition tests must continue rejecting duplicates within one request.
