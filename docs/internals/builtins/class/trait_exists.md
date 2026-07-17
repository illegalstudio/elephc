---
title: "trait_exists() — internals"
description: "Compiler internals for trait_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 88
---

## `trait_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/trait_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/trait_exists.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:581](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L581) (`lower_class_like_exists`)
- **Function symbol**: `lower_class_like_exists()`


### Lowering notes

- Lowers AOT class/interface/enum existence checks for literal or dynamic string names.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function trait_exists(string $trait, bool $autoload = true): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/trait_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/trait_exists.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `trait_exists()`](../../../php/builtins/class/trait_exists.md)
