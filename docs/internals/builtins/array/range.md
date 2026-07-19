---
title: "range() — internals"
description: "Compiler internals for range(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 57
---

## `range()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/range.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/range.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1023](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1023) (`lower_range`)
- **Function symbol**: `lower_range()`


### Lowering notes

- Lowers `range()` for integer endpoints through the shared runtime constructor.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_int`
- `__rt_range`

## Signature summary

```php
function range(mixed $start, mixed $end): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/range.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/range.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `range()`](../../../php/builtins/array/range.md)
