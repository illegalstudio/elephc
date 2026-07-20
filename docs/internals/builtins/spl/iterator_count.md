---
title: "iterator_count() — internals"
description: "Compiler internals for iterator_count(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 339
---

## `iterator_count()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/iterator_count.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/iterator_count.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/spl.rs`:237](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/spl.rs#L237) (`lower_iterator_count`)
- **Function symbol**: `lower_iterator_count()`


### Lowering notes

- Lowers `iterator_count()` over arrays, `iterable`, and Traversable objects.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function iterator_count(traversable $iterator): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/iterator_count.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/iterator_count.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `iterator_count()`](../../../php/builtins/spl/iterator_count.md)
