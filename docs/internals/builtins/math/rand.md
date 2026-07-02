---
title: "rand() — internals"
description: "Compiler internals for rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 263
---

## `rand()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/rand.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/rand.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/random.rs`:21](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/random.rs#L21) (`lower_rand`)
- **Function symbol**: `lower_rand()`


### Lowering notes

- Lowers `rand()` and `mt_rand()` with either zero args or an inclusive range.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_random_u32`

## Signature summary

```php
function rand(int $min, int $max): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `rand()`](../../../php/builtins/math/rand.md)

