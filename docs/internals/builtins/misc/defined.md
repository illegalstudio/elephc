---
title: "defined() — internals"
description: "Compiler internals for defined(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 290
---

## `defined()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/defined.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/defined.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:553](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L553) (`lower_defined`)
- **Function symbol**: `lower_defined()`


### Lowering notes

- Lowers `defined("NAME")` for compile-time string constant names.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function defined(string $constant_name): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/defined.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/defined.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `defined()`](../../../php/builtins/misc/defined.md)
