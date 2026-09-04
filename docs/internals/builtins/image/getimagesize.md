---
title: "getimagesize() — internals"
description: "Compiler internals for getimagesize(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 451
---

## `getimagesize()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3833](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3833) (`getimagesize`)
- **Function symbol**: `getimagesize()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function getimagesize(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `getimagesize()`](../../../php/builtins/image/getimagesize.md)
