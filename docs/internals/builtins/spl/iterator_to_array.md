---
title: "iterator_to_array() — internals"
description: "Compiler internals for iterator_to_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 321
---

## `iterator_to_array()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:265](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L265) (`lower_iterator_to_array`)
- **Function symbol**: `lower_iterator_to_array()`


### Lowering notes

- Lowers `iterator_to_array()` over arrays, `iterable`, and Traversable objects.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function iterator_to_array(traversable $iterator, bool $preserve_keys): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `iterator_to_array()`](../../../php/builtins/spl/iterator_to_array.md)

