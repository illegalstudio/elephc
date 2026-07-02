---
title: "random_int() — internals"
description: "Compiler internals for random_int(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 263
---

## `random_int()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/random.rs`:40](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/random.rs#L40) (`lower_random_int`)
- **Function symbol**: `lower_random_int()`


### Lowering notes

- Lowers `random_int()` over an inclusive integer range.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_random_bytes`

## Signature summary

```php
function random_int(int $min, int $max): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `random_int()`](../../../php/builtins/math/random_int.md)

