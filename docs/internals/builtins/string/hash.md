---
title: "hash() — internals"
description: "Compiler internals for hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 352
---

## `hash()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/hash.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/hash.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:227](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L227) (`lower_hash`)
- **Function symbol**: `lower_hash()`


### Lowering notes

- Lowers `hash(algo, data, binary?)` through the shared runtime digest dispatcher.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash`

## Signature summary

```php
function hash(string $algo, string $data, bool $binary = false): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `hash()`](../../../php/builtins/string/hash.md)
