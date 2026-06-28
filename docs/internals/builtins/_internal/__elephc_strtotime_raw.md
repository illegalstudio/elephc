---
title: "__elephc_strtotime_raw() — internals"
description: "Compiler internals for __elephc_strtotime_raw(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 256
---

## `__elephc_strtotime_raw()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:543](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L543) (`lower_elephc_strtotime_raw`)
- **Function symbol**: `lower_elephc_strtotime_raw()`


### Lowering notes

- Internal helper used by the strtotime() builtin.
- Provides a raw timestamp parsing path for the runtime strtotime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strtotime`

## Signature summary

```php
function __elephc_strtotime_raw(string $datetime, int $baseTimestamp): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- _No user-facing reference — this is a compiler internal helper._

