---
title: "imagecreatefrompng() — internals"
description: "Compiler internals for imagecreatefrompng(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 489
---

## `imagecreatefrompng()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3385](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3385) (`imagecreatefrompng`)
- **Function symbol**: `imagecreatefrompng()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecreatefrompng(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecreatefrompng()`](../../../php/builtins/image/imagecreatefrompng.md)
