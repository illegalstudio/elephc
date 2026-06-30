---
title: "is_callable() — internals"
description: "Compiler internals for is_callable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 420
---

## `is_callable()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:844](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L844) (`lower_is_callable`)
- **Function symbol**: `lower_is_callable()`


### Lowering notes

- Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_callable_array`
- `__rt_is_callable_assoc`
- `__rt_is_callable_object`

## Signature summary

```php
function is_callable(mixed $value, bool $syntax_only = false, string $callable_name = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).
- **By-reference parameters**: `$callable_name`.

## Cross-references

- [User reference for `is_callable()`](../../../php/builtins/type/is_callable.md)

