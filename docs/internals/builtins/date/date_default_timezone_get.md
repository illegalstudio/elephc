---
title: "date_default_timezone_get() — internals"
description: "Compiler internals for date_default_timezone_get(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 70
---

## `date_default_timezone_get()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:70](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L70) (`lower_date_default_timezone_get`)
- **Function symbol**: `lower_date_default_timezone_get()`


### Lowering notes

- Lowers `date_default_timezone_get()` through the shared runtime helper.
- Takes no arguments; `__rt_date_default_timezone_get` returns the stored timezone
- identifier (or the literal `"UTC"` when none was set) in the string-result registers.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_date_default_timezone_get`
- `__rt_date_default_timezone_set`

## Signature summary

```php
function date_default_timezone_get(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `date_default_timezone_get()`](../../../php/builtins/date/date_default_timezone_get.md)

