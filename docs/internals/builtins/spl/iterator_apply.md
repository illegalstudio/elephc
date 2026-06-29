---
title: "iterator_apply() — internals"
description: "Compiler internals for iterator_apply(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 304
---

## `iterator_apply()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:289](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L289) (`lower_iterator_apply`)
- **Function symbol**: `lower_iterator_apply()`


### Lowering notes

- Lowers `iterator_apply()` over supported Traversable sources and callback forms.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function iterator_apply(traversable $iterator, callable $callback, array $args): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `iterator_apply()`](../../../php/builtins/spl/iterator_apply.md)

