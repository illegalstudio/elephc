---
title: "is_callable() — internals"
description: "Compiler internals for is_callable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 427
---

## `is_callable()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/is_callable.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/is_callable.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:710](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L710) (`lower_is_callable`)
- **Function symbol**: `lower_is_callable()`


### Lowering notes

- Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_callable_array`
- `__rt_is_callable_assoc`

## Signature summary

```php
function is_callable(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/is_callable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/is_callable.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$callable_name`.

## Cross-references

- [User reference for `is_callable()`](../../../php/builtins/type/is_callable.md)
