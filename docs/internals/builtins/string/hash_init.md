---
title: "hash_init() — internals"
description: "Compiler internals for hash_init(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 339
---

## `hash_init()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:266](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L266) (`lower_hash_init`)
- **Function symbol**: `lower_hash_init()`


### Lowering notes

- Lowers `hash_init(algo)` and returns a boxed HashContext resource.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_init`

## Signature summary

```php
function hash_init(string $algo, int $flags = 0, string $key = '', array $options = []): mixed
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Cross-references

- [User reference for `hash_init()`](../../../php/builtins/string/hash_init.md)

