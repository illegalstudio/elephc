---
title: "phpversion() — internals"
description: "Compiler internals for phpversion(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 283
---

## `phpversion()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/phpversion.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/phpversion.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:539](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L539) (`lower_phpversion`)
- **Function symbol**: `lower_phpversion()`


### Lowering notes

- Lowers `phpversion()` as the compiler package version string.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function phpversion(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/phpversion.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/phpversion.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `phpversion()`](../../../php/builtins/misc/phpversion.md)
