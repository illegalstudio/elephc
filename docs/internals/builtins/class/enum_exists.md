---
title: "enum_exists() — internals"
description: "Compiler internals for enum_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 74
---

## `enum_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/enum_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/enum_exists.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:294](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L294) (`lower_class_like_exists`)
- **Function symbol**: `lower_class_like_exists()`


### Lowering notes

- Lowers AOT class/interface/enum existence checks for literal names.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function enum_exists(string $enum, bool $autoload = true): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `enum_exists()`](../../../php/builtins/class/enum_exists.md)
