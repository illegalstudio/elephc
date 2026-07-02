---
title: "is_a() — internals"
description: "Compiler internals for is_a(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 82
---

## `is_a()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/is_a.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/is_a.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/types.rs`:369](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/types.rs#L369) (`lower_is_a_relation`)
- **Function symbol**: `lower_is_a_relation()`


### Lowering notes

- Lowers `is_a()` and `is_subclass_of()` for object operands and literal targets.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_a(object $object_or_class, string $class, bool $allow_string = false): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `is_a()`](../../../php/builtins/class/is_a.md)

