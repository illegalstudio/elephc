---
title: "class_attribute_names() — internals"
description: "Compiler internals for class_attribute_names(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 51
---

## `class_attribute_names()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/attributes.rs`:36](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/attributes.rs#L36) (`lower_class_attribute_names`)
- **Function symbol**: `lower_class_attribute_names()`


### Lowering notes

- Lowers `class_attribute_names(class)` into an indexed string array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_attribute_names(string $class_name): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `class_attribute_names()`](../../../php/builtins/class/class_attribute_names.md)

