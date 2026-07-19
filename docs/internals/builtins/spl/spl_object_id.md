---
title: "spl_object_id() — internals"
description: "Compiler internals for spl_object_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 349
---

## `spl_object_id()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/spl_object_id.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/spl_object_id.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/spl.rs`:216](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/spl.rs#L216) (`lower_spl_object_id`)
- **Function symbol**: `lower_spl_object_id()`


### Lowering notes

- Lowers `spl_object_id(object)` by returning the loaded object pointer as an integer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_itoa`

## Signature summary

```php
function spl_object_id(object $object): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/spl_object_id.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_object_id.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `spl_object_id()`](../../../php/builtins/spl/spl_object_id.md)
