---
title: "imagecreatefromgif() — internals"
description: "Compiler internals for imagecreatefromgif(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 487
---

## `imagecreatefromgif()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3425](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3425) (`imagecreatefromgif`)
- **Function symbol**: `imagecreatefromgif()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecreatefromgif(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecreatefromgif()`](../../../php/builtins/image/imagecreatefromgif.md)
