---
title: "checkdate() — internals"
description: "Compiler internals for checkdate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 68
---

## `checkdate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:163](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L163) (`lower_checkdate`)
- **Function symbol**: `lower_checkdate()`


### Lowering notes

- Lowers `checkdate(month, day, year)` through the shared Gregorian-validation runtime helper.
- Marshals the three integers into the leading ABI argument registers (unboxing any boxed
- `Mixed`/`Union` argument), then calls `__rt_checkdate`, which returns PHP `true`/`false` in the
- integer result register for a valid/invalid date.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_checkdate`
- `__rt_getdate`

## Signature summary

```php
function checkdate(int $month, int $day, int $year): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `checkdate()`](../../../php/builtins/date/checkdate.md)

