---
title: "__elephc_mktime_raw() — internals"
description: "Compiler internals for __elephc_mktime_raw(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 255
---

## `__elephc_mktime_raw()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:140](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L140) (`lower_mktime`)
- **Function symbol**: `lower_mktime()`


### Lowering notes

- Internal helper used by the mktime() builtin.
- Bypasses timezone handling and calls the runtime mktime helper directly.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_checkdate`
- `__rt_gmmktime`
- `__rt_mktime`

## Signature summary

```php
function __elephc_mktime_raw(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._

