---
title: "uksort() — internals"
description: "Compiler internals for uksort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 45
---

## `uksort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1126](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1126) (`lower_uksort`)
- **Function symbol**: `lower_uksort()`


### Lowering notes

- Lowers `uksort()` through the legacy user-sort helper for static comparators.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function uksort(array $array, callable $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `uksort()`](../../../php/builtins/array/uksort.md)

