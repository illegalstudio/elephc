---
title: "dir() — internals"
description: "Compiler internals for dir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 167
---

## `dir()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/hash_prelude.rs`:1](https://github.com/illegalstudio/elephc/blob/main/src/hash_prelude.rs#L1) (`dir`)
- **Function symbol**: `dir()`


### Lowering notes

- Implemented by a compiler-injected PHP prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function dir(string $directory, mixed $context = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `dir()`](../../../php/builtins/io/dir.md)
