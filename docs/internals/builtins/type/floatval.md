---
title: "floatval() — internals"
description: "Compiler internals for floatval(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 398
---

## `floatval()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:1061](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L1061) (`lower_floatval`)
- **Function symbol**: `lower_floatval()`


### Lowering notes

- Lowers `floatval()` for concrete scalar operands.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_to_number`

## Signature summary

```php
function floatval(mixed $value): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `floatval()`](../../../php/builtins/type/floatval.md)

