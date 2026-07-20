---
title: "random_int() — internals"
description: "Compiler internals for random_int(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 281
---

## `random_int()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/random_int.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/random_int.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/random.rs`:40](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/random.rs#L40) (`lower_random_int`)
- **Function symbol**: `lower_random_int()`


### Lowering notes

- Lowers `random_int()` over an inclusive integer range.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function random_int(int $min, int $max): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/random_int.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/random_int.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `random_int()`](../../../php/builtins/math/random_int.md)
