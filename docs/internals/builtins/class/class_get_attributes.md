---
title: "class_get_attributes() — internals"
description: "Compiler internals for class_get_attributes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 53
---

## `class_get_attributes()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/attributes.rs`:68](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/attributes.rs#L68) (`lower_class_get_attributes`)
- **Function symbol**: `lower_class_get_attributes()`


### Lowering notes

- Lowers `class_get_attributes(class)` into an array of `ReflectionAttribute` objects.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_get_attributes(string $class_name): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `class_get_attributes()`](../../../php/builtins/class/class_get_attributes.md)

