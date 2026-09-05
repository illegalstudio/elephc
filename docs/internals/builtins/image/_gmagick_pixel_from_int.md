---
title: "_gmagick_pixel_from_int() — internals"
description: "Compiler internals for _gmagick_pixel_from_int(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 965
---

## `_gmagick_pixel_from_int()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:10193](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L10193) (`_gmagick_pixel_from_int`)
- **Function symbol**: `_gmagick_pixel_from_int()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function _gmagick_pixel_from_int(int $packed): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
