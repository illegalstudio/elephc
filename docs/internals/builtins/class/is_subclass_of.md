---
title: "is_subclass_of() — internals"
description: "Compiler internals for is_subclass_of(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 83
---

## `is_subclass_of()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/is_subclass_of.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/is_subclass_of.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/types.rs`:369](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/types.rs#L369) (`lower_is_a_relation`)
- **Function symbol**: `lower_is_a_relation()`


### Lowering notes

- Lowers `is_a()` and `is_subclass_of()` for object, boxed-Mixed, and string-class-name operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_subclass_of(mixed $object_or_class, string $class, bool $allow_string = true): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `is_subclass_of()`](../../../php/builtins/class/is_subclass_of.md)
