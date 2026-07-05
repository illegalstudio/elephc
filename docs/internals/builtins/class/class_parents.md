---
title: "class_parents() — internals"
description: "Compiler internals for class_parents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 72
---

## `class_parents()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/class_parents.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/class_parents.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/class_relations.rs`:32](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/class_relations.rs#L32) (`lower_class_relation`)
- **Function symbol**: `lower_class_relation()`


### Lowering notes

- Lowers `class_implements()`, `class_parents()`, and `class_uses()` from static metadata.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_parents(mixed $object_or_class, bool $autoload = true): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `class_parents()`](../../../php/builtins/class/class_parents.md)
