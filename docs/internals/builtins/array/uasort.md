---
title: "uasort() — internals"
description: "Compiler internals for uasort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 44
---

## `uasort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1131](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1131) (`lower_uasort`)
- **Function symbol**: `lower_uasort()`


### Lowering notes

- Lowers `uasort()` through the legacy user-sort helper for static comparators.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function uasort(array $array, callable $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `uasort()`](../../../php/builtins/array/uasort.md)

