---
title: "iterator_count() — internals"
description: "Compiler internals for iterator_count(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 320
---

## `iterator_count()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:236](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L236) (`lower_iterator_count`)
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

## Cross-references

- [User reference for `iterator_count()`](../../../php/builtins/spl/iterator_count.md)

