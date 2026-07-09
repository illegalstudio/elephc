---
title: "hash_init() — internals"
description: "Compiler internals for hash_init(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 358
---

## `hash_init()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/hash_init.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/hash_init.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:266](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L266) (`lower_hash_init`)
- **Function symbol**: `lower_hash_init()`


### Lowering notes

- Lowers `hash_init(algo)` and returns a boxed HashContext resource.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_init`

## Signature summary

```php
function hash_init(string $algo, int $flags = 0, string $key = ''): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `hash_init()`](../../../php/builtins/string/hash_init.md)
