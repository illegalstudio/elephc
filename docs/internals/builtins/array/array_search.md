---
title: "array_search() — internals"
description: "Compiler internals for array_search(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 23
---

## `array_search()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1141](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1141) (`lower_array_search`)
- **Function symbol**: `lower_array_search()`


### Lowering notes

- Lowers `array_search()` for indexed arrays with integer-like payloads.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_search(mixed $needle, array $haystack, bool $strict): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `array_search()`](../../../php/builtins/array/array_search.md)

