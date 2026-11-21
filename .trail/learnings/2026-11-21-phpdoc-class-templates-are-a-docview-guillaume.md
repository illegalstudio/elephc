---
title: "PHPDoc class templates are a DocView overlay, not monomorphized"
author: Guillaume
date: 2026-11-21
branch: bead-erasure2
touches:
  - src/docblock.rs
  - src/docblock/members.rs
  - src/generics/classes.rs
  - src/generics/stats.rs
---

## Decided
- A class/interface/trait whose type parameters ALL come from PHPDoc (`@template`) stays exactly as written: one class, native types, no marker interface. `docblock::apply` records its PHPDoc reading as a `DocView` (type params, `@extends`/`@implements` args) in a side table (`docblock::doc_view`, `is_doc_template`). Nothing consumes it yet (beads elephc-h17m.92.1-.92.5).
- `generics::classes::collect` therefore sees no template (`generics` is None); mentions such as `@extends Box<T>` are forgotten via `is_doc_template`. PHPDoc function/method templates and native `<T>` classes are unchanged; `rerun_erasing` stays as a safety net.
- php -n decides: tests that asserted per-instantiation specialization of a PHPDoc class were realigned (no bound warning, `object` instead of the bound in messages, literal rejects are compile-time errors so fixtures use json_decode values).

## Tried and ruled out
- Making PhpType carry type arguments: 1412 `Object(` sites, the checker infers representations so per-instance checking is intrinsic; one multi-layout class is unsound through mixed.
- Erasing from a pre-scan or in-process: still pays the instantiation machinery.

## Watch out
- Root cause of the 4 erasure rounds (~600 s user each): generics are AST copies (one ClassDecl per `Name<args>`) and PhpType has no type arguments, so every doc instantiation re-checks the whole program and every refusal re-execs the compiler. Plan: hindsight initiative kp-96b174fc8ba8432280f1522f07ef374b.
- A mixed value into a variadic `object ...$x` hint segfaults and `?iterable` accepts mixed unchecked on a plain class (elephc-h17m.94).
