---
title: "mktime() — internals"
description: "Compiler internals for mktime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 78
---

## `mktime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:140](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L140) (`lower_mktime`)
- **Function symbol**: `lower_mktime()`


### Lowering notes

- Lowers `mktime(hour, minute, second, month, day, year)` through the runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_checkdate`
- `__rt_gmmktime`
- `__rt_mktime`

## Signature summary

```php
function mktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Cross-references

- [User reference for `mktime()`](../../../php/builtins/date/mktime.md)

