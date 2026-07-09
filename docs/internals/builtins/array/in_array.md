---
title: "in_array() — internals"
description: "Compiler internals for in_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 52
---

## `in_array()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/in_array.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/in_array.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1786](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1786) (`lower_in_array`)
- **Function symbol**: `lower_in_array()`


### Lowering notes

- Lowers `in_array()` for indexed and associative arrays with PHP loose or strict membership.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function in_array(mixed $needle, array $haystack, bool $strict = false): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `in_array()`](../../../php/builtins/array/in_array.md)
