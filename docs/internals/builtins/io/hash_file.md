---
title: "hash_file() — internals"
description: "Compiler internals for hash_file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 167
---

## `hash_file()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:180](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L180) (`lower_hash_file`)
- **Function symbol**: `lower_hash_file()`


### Lowering notes

- Lowers `hash_file(algo, filename, binary?)` by reading bytes then hashing them.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function hash_file(string $algo, string $filename, bool $binary = false, array $options = []): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `hash_file()`](../../../php/builtins/io/hash_file.md)

