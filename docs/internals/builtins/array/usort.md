---
title: "usort() — internals"
description: "Compiler internals for usort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 46
---

## `usort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1121](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1121) (`lower_usort`)
- **Function symbol**: `lower_usort()`


### Lowering notes

- Lowers `usort()` for indexed integer arrays with a static user comparator.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function usort(array $array, callable $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `usort()`](../../../php/builtins/array/usort.md)

