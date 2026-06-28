---
title: "date() — internals"
description: "Compiler internals for date(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 69
---

## `date()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L22) (`lower_date`)
- **Function symbol**: `lower_date()`


### Lowering notes

- Lowers `date(format, timestamp?)` through the shared formatter runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_date`
- `__rt_gmdate`

## Signature summary

```php
function date(string $format, int $timestamp): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `date()`](../../../php/builtins/date/date.md)

