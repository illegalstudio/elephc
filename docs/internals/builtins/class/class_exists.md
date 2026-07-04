---
title: "class_exists() — internals"
description: "Compiler internals for class_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 69
---

## `class_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/class_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/class_exists.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:294](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L294) (`lower_class_like_exists`)
- **Function symbol**: `lower_class_like_exists()`


### Lowering notes

- Lowers AOT class/interface/enum existence checks for literal names.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_exists(string $class, bool $autoload = true): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `class_exists()`](../../../php/builtins/class/class_exists.md)

