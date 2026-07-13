---
title: "class_exists() — internals"
description: "Compiler internals for class_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 69
---

## `class_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/class_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/class_exists.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:583](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L583) (`lower_class_like_exists`)
- **Function symbol**: `lower_class_like_exists()`


### Lowering notes

- Lowers AOT class/interface/enum existence checks for literal or dynamic string names.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_exists(string $class, bool $autoload = true): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `class_exists()`](../../../php/builtins/class/class_exists.md)
