---
title: "hash_copy() — internals"
description: "Compiler internals for hash_copy(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 371
---

## `hash_copy()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/hash_copy.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/hash_copy.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:350](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L350) (`lower_hash_copy`)
- **Function symbol**: `lower_hash_copy()`


### Lowering notes

- Lowers `hash_copy(context)` through the incremental hash clone helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_crc32`
- `__rt_hash_copy`

## Signature summary

```php
function hash_copy(resource $context): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_copy()`](../../../php/builtins/string/hash_copy.md)
