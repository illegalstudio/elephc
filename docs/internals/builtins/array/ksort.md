---
title: "ksort() — internals"
description: "Compiler internals for ksort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 52
---

## `ksort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1272](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1272) (`lower_ksort`)
- **Function symbol**: `lower_ksort()`


### Lowering notes

- Lowers `ksort()` through the legacy key-sort helper surface.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_krsort`
- `__rt_ksort`
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function ksort(array $value, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ksort()`](../../../php/builtins/array/ksort.md)

