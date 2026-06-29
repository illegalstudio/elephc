---
title: "spl_object_id() — internals"
description: "Compiler internals for spl_object_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 315
---

## `spl_object_id()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:215](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L215) (`lower_spl_object_id`)
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

## Cross-references

- [User reference for `spl_object_id()`](../../../php/builtins/spl/spl_object_id.md)

