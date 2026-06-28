---
title: "range() — internals"
description: "Compiler internals for range(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 40
---

## `range()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1020](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1020) (`lower_range`)
- **Function symbol**: `lower_range()`


### Lowering notes

- Lowers `range()` for integer endpoints through the shared runtime constructor.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_int`
- `__rt_range`

## Signature summary

```php
function range(mixed $start, mixed $end, int $step): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `range()`](../../../php/builtins/array/range.md)

