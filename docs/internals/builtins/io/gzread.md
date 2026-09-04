---
title: "gzread() — internals"
description: "Compiler internals for gzread(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 208
---

## `gzread()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:116](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L116) (`gzread`)
- **Function symbol**: `gzread()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzread(mixed $stream, int $length): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzread()`](../../../php/builtins/io/gzread.md)
