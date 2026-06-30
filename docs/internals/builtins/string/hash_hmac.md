---
title: "hash_hmac() — internals"
description: "Compiler internals for hash_hmac(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 355
---

## `hash_hmac()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:228](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L228) (`lower_hash_hmac`)
- **Function symbol**: `lower_hash_hmac()`


### Lowering notes

- Lowers `hash_hmac(algo, data, key, binary?)` through the shared HMAC runtime dispatcher.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_equals`
- `__rt_hash_hmac`

## Signature summary

```php
function hash_hmac(string $algo, string $data, string $key, bool $binary): string
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).

## Cross-references

- [User reference for `hash_hmac()`](../../../php/builtins/string/hash_hmac.md)

