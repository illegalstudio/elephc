---
title: "class_attribute_args() — internals"
description: "Compiler internals for class_attribute_args(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 67
---

## `class_attribute_args()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/class_attribute_args.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/class_attribute_args.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/attributes.rs`:61](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/attributes.rs#L61) (`lower_class_attribute_args`)
- **Function symbol**: `lower_class_attribute_args()`


### Lowering notes

- Lowers `class_attribute_args(class, attr)` into a Mixed PHP argument array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_attribute_args(string $class_name, string $attribute_name): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/class_attribute_args.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_attribute_args.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `class_attribute_args()`](../../../php/builtins/class/class_attribute_args.md)
