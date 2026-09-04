---
title: "imagecopymerge() — internals"
description: "Compiler internals for imagecopymerge(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 481
---

## `imagecopymerge()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3009](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3009) (`imagecopymerge`)
- **Function symbol**: `imagecopymerge()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecopymerge(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool
```

## What the type checker enforces

- **Arity**: takes exactly 9 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecopymerge()`](../../../php/builtins/image/imagecopymerge.md)
