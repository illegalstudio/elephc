---
title: "hash_file() — internals"
description: "Compiler internals for hash_file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 188
---

## `hash_file()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/hash_file.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/hash_file.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:287](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L287) (`lower_hash_file`)
- **Function symbol**: `lower_hash_file()`


### Lowering notes

- Lowers `hash_file(algo, filename, binary?)` by reading bytes then hashing them.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_vd_write`

## Signature summary

```php
function hash_file(string $algo, string $filename, bool $binary = false): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_file.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_file()`](../../../php/builtins/io/hash_file.md)
