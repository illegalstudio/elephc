---
title: "count() — internals"
description: "Compiler internals for count(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 34
---

## `count()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:944](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L944) (`lower_count`)
- **Function symbol**: `lower_count()`


### Lowering notes

- Lowers `count(array)` for concrete array values by reading the runtime length header.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_count`

## Signature summary

```php
function count(array $value, int $mode): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `count()`](../../../php/builtins/array/count.md)

