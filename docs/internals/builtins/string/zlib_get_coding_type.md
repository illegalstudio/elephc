---
title: "zlib_get_coding_type() — internals"
description: "Compiler internals for zlib_get_coding_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 554
---

## `zlib_get_coding_type()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:257](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L257) (`zlib_get_coding_type`)
- **Function symbol**: `zlib_get_coding_type()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function zlib_get_coding_type(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zlib_get_coding_type()`](../../../php/builtins/string/zlib_get_coding_type.md)
