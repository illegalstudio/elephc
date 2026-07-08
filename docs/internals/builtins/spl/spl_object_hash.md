---
title: "spl_object_hash() — internals"
description: "Compiler internals for spl_object_hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 330
---

## `spl_object_hash()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/spl_object_hash.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/spl_object_hash.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/spl.rs`:226](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/spl.rs#L226) (`lower_spl_object_hash`)
- **Function symbol**: `lower_spl_object_hash()`


### Lowering notes

- Lowers `spl_object_hash(object)` by formatting the loaded object pointer as a string.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_itoa`

## Signature summary

```php
function spl_object_hash(object $object): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `spl_object_hash()`](../../../php/builtins/spl/spl_object_hash.md)
