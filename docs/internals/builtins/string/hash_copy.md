---
title: "hash_copy() — internals"
description: "Compiler internals for hash_copy(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 352
---

## `hash_copy()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:333](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L333) (`lower_hash_copy`)
- **Function symbol**: `lower_hash_copy()`


### Lowering notes

- Lowers `hash_copy(context)` through the incremental hash clone helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_crc32`
- `__rt_hash_copy`
- `__rt_md5`
- `__rt_sha1`

## Signature summary

```php
function hash_copy(resource $context): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `hash_copy()`](../../../php/builtins/string/hash_copy.md)

