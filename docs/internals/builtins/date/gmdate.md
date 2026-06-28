---
title: "gmdate() — internals"
description: "Compiler internals for gmdate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 73
---

## `gmdate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:33](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L33) (`lower_gmdate`)
- **Function symbol**: `lower_gmdate()`


### Lowering notes

- Lowers `gmdate(format[, timestamp])`: the UTC counterpart of `date()`.
- Identical argument marshalling to `date()`, but dispatches to `__rt_gmdate`, which formats
- the instant in UTC regardless of the active default timezone.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_date`
- `__rt_gmdate`

## Signature summary

```php
function gmdate(string $format, int $timestamp): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `gmdate()`](../../../php/builtins/date/gmdate.md)

