---
title: "class_alias() — internals"
description: "Compiler internals for class_alias(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 49
---

## `class_alias()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/types.rs`:41](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/types.rs#L41) (`lower_class_alias`)
- **Function symbol**: `lower_class_alias()`


### Lowering notes

- Lowers the defensive `class_alias()` fallback that remains after AOT alias extraction.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_alias(string $class, string $alias, bool $autoload): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `class_alias()`](../../../php/builtins/class/class_alias.md)

