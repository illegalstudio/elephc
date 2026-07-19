---
title: "empty() — internals"
description: "Compiler internals for empty(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 291
---

## `empty()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1233](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1233) (`lower_empty`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `empty()`](../../../php/builtins/misc/empty.md)
