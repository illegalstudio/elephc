---
title: "is_array() — internals"
description: "Compiler internals for is_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 438
---

## `is_array()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/is_array.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/is_array.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1621](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1621) (`lower_is_array`)
- **Function symbol**: `lower_is_array()`


### Lowering notes

- Lowers `is_array()`: true for statically-known arrays/hashes, or a boxed Mixed/Union value
- whose runtime tag is an indexed (4) or associative (5) array. An `iterable`-typed value is
- not treated as a definite array here (it may hold a Traversable); use `is_iterable` for that.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_array(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/is_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_array.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_array()`](../../../php/builtins/type/is_array.md)
