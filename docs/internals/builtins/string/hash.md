---
title: "hash() — internals"
description: "Compiler internals for hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 333
---

## `hash()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:209](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L209) (`lower_hash`)
- **Function symbol**: `lower_hash()`


### Lowering notes

- Lowers `hash(algo, data, binary?)` through the shared runtime digest dispatcher.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash`

## Signature summary

```php
function hash(string $algo, string $data, bool $binary = false, array $options = []): string
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `hash()`](../../../php/builtins/string/hash.md)

