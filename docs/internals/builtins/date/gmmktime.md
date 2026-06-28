---
title: "gmmktime() — internals"
description: "Compiler internals for gmmktime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 74
---

## `gmmktime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:151](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L151) (`lower_gmmktime`)
- **Function symbol**: `lower_gmmktime()`


### Lowering notes

- Lowers `gmmktime(...)`: the UTC counterpart of `mktime()`.
- Identical six-integer argument marshalling, but dispatches to `__rt_gmmktime`, which
- interprets the broken-down date/time as UTC (`timegm`) instead of local time.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_checkdate`
- `__rt_getdate`
- `__rt_gmmktime`

## Signature summary

```php
function gmmktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Cross-references

- [User reference for `gmmktime()`](../../../php/builtins/date/gmmktime.md)

