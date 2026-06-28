---
title: "empty() — internals"
description: "Compiler internals for empty(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 262
---

## `empty()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:1074](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L1074) (`lower_empty`)
- **Function symbol**: `lower_empty()`


### Lowering notes

- Lowers `empty()` for concrete scalar and array-like operands.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_is_empty`

## Signature summary

```php
function empty(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `empty()`](../../../php/builtins/misc/empty.md)

