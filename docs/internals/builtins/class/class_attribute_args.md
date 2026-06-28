---
title: "class_attribute_args() — internals"
description: "Compiler internals for class_attribute_args(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 50
---

## `class_attribute_args()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/attributes.rs`:52](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/attributes.rs#L52) (`lower_class_attribute_args`)
- **Function symbol**: `lower_class_attribute_args()`


### Lowering notes

- Lowers `class_attribute_args(class, attr)` into an indexed Mixed array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_attribute_args(string $class_name, string $attribute_name): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `class_attribute_args()`](../../../php/builtins/class/class_attribute_args.md)

