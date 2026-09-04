---
title: "gzseek() — internals"
description: "Compiler internals for gzseek(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 210
---

## `gzseek()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:139](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L139) (`gzseek`)
- **Function symbol**: `gzseek()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzseek(mixed $stream, int $offset, int $whence = 0): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzseek()`](../../../php/builtins/io/gzseek.md)
