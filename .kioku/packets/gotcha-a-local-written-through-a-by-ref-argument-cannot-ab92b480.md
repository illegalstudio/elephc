---
id: gotcha-a-local-written-through-a-by-ref-argument-cannot-ab92b480
type: gotcha
title: "A local written through a by-ref argument cannot be marked gradual"
description: "mixed_storage_scan deliberately disqualifies by-ref call arguments from boxed storage, so Symfony RedisTrait preg_match-then-assign on the same local needs a storage change not a scan tweak"
created: 2026-09-17
sources:
  - path: src/types/checker/mixed_storage_scan.rs
    blob: d91ac04ccd234a4d1120474642cf09bf912ed89a
superseded_by: "bug_fix-a-by-reference-capture-whose-closure-writes-it-i-b2f8f873"
superseded_why: "Fixed for the capture case: lowering had already boxed that cell, so the checker was stricter than the storage"
---

# A local written through a by-ref argument cannot be marked gradual

## Fact

Symfony's `RedisTrait` line 332:

    $error = preg_match('/^Redis::p?connect\(\): (.*)/', $error ?? ... ?? '', $error)
        ? \sprintf(' (%s)', $error[1]) : '';

elephc: `Type error: cannot reassign $error from array<mixed> to string`. PHP runs it: the by-ref
third argument writes the matches array, the ternary READS `$error[1]`, then the assignment stores a
string.

The obvious fix — let `mixed_storage_scan` mark `$error` as boxed `Mixed` — is blocked by design:
`collect_expr`'s call arms call `disqualify_call_arguments`, deliberately, because "a by-reference
parameter anywhere behind them would alias the local for the rest of the body" and re-binding it to
boxed storage would break that alias.

So the slot genuinely holds two types AND is aliased by reference. Closing it needs the boxed slot
and the by-ref alias to agree (the alias must point AT the box), which is a storage change rather
than a scan tweak.

## Why

The disqualification is load-bearing, so this blocker needs a storage change, not a scan tweak
