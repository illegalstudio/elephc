---
title: "hash_final() — internals"
description: "Compiler internals for hash_final(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 354
---

## `hash_final()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:302](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L302) (`lower_hash_final`)
- **Function symbol**: `lower_hash_final()`


### Lowering notes

- Lowers `hash_final(context, binary?)` through the incremental hash finalizer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_final`

## Signature summary

```php
function hash_final(resource $context, bool $binary): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `hash_final()`](../../../php/builtins/string/hash_final.md)

