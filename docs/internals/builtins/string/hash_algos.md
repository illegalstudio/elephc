---
title: "hash_algos() — internals"
description: "Compiler internals for hash_algos(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 351
---

## `hash_algos()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:254](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L254) (`lower_hash_algos`)
- **Function symbol**: `lower_hash_algos()`


### Lowering notes

- Lowers `hash_algos()` through the runtime algorithm-list builder.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_algos_list`
- `__rt_hash_init`

## Signature summary

```php
function hash_algos(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `hash_algos()`](../../../php/builtins/string/hash_algos.md)

