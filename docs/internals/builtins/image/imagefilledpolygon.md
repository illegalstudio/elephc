---
title: "imagefilledpolygon() — internals"
description: "Compiler internals for imagefilledpolygon(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 502
---

## `imagefilledpolygon()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2875](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2875) (`imagefilledpolygon`)
- **Function symbol**: `imagefilledpolygon()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagefilledpolygon(mixed $image, mixed $points, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagefilledpolygon()`](../../../php/builtins/image/imagefilledpolygon.md)
