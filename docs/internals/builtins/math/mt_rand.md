---
title: "mt_rand() — internals"
description: "Compiler internals for mt_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 259
---

## `mt_rand()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/mt_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/mt_rand.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/random.rs`:21](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/random.rs#L21) (`lower_rand`)
- **Function symbol**: `lower_rand()`


### Lowering notes

- Lowers `rand()` and `mt_rand()` with either zero args or an inclusive range.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_random_u32`

## Signature summary

```php
function mt_rand(int $min, int $max): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `mt_rand()`](../../../php/builtins/math/mt_rand.md)
