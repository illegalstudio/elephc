---
title: "gzputs() — internals"
description: "Compiler internals for gzputs(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 207
---

## `gzputs()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:127](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L127) (`gzputs`)
- **Function symbol**: `gzputs()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzputs(mixed $stream, string $data, int $length = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzputs()`](../../../php/builtins/io/gzputs.md)
