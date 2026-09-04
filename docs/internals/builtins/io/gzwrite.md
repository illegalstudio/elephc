---
title: "gzwrite() — internals"
description: "Compiler internals for gzwrite(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 212
---

## `gzwrite()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:120](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L120) (`gzwrite`)
- **Function symbol**: `gzwrite()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzwrite(mixed $stream, string $data, int $length = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzwrite()`](../../../php/builtins/io/gzwrite.md)
