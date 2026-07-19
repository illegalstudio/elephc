---
title: "__elephc_strtotime_raw() — internals"
description: "Compiler internals for __elephc_strtotime_raw(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 468
---

## `__elephc_strtotime_raw()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_strtotime_raw.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_strtotime_raw.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:543](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L543) (`lower_elephc_strtotime_raw`)
- **Function symbol**: `lower_elephc_strtotime_raw()`


### Lowering notes

- Internal helper used by the strtotime() builtin.
- Provides a raw timestamp parsing path for the runtime strtotime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_strtotime_raw(string $datetime, int $baseTimestamp = null): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
