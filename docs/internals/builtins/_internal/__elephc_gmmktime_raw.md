---
title: "__elephc_gmmktime_raw() — internals"
description: "Compiler internals for __elephc_gmmktime_raw(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 254
---

## `__elephc_gmmktime_raw()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:151](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L151) (`lower_gmmktime`)
- **Function symbol**: `lower_gmmktime()`


### Lowering notes

- Internal helper used by the gmmktime() builtin.
- Bypasses timezone handling and calls the runtime gmmktime helper directly.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_checkdate`
- `__rt_getdate`
- `__rt_gmmktime`

## Signature summary

```php
function __elephc_gmmktime_raw(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._

