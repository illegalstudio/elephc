---
title: "trait_exists() — internals"
description: "Compiler internals for trait_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 84
---

## `trait_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/trait_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/trait_exists.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:296](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L296) (`lower_class_like_exists`)
- **Function symbol**: `lower_class_like_exists()`


### Lowering notes

- Lowers AOT class/interface/enum existence checks for literal names.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function trait_exists(string $trait, bool $autoload = true): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `trait_exists()`](../../../php/builtins/class/trait_exists.md)
