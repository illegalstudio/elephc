---
title: "zlib_encode() — internals"
description: "Compiler internals for zlib_encode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 553
---

## `zlib_encode()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:181](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L181) (`zlib_encode`)
- **Function symbol**: `zlib_encode()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function zlib_encode(string $data, int $encoding, int $level = -1): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zlib_encode()`](../../../php/builtins/string/zlib_encode.md)
