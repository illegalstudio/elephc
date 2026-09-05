---
title: "exif_thumbnail() — internals"
description: "Compiler internals for exif_thumbnail(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 449
---

## `exif_thumbnail()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3987](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3987) (`exif_thumbnail`)
- **Function symbol**: `exif_thumbnail()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function exif_thumbnail(string $filename, mixed $width = 0, mixed $height = 0, mixed $image_type = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).
- **By-reference parameters**: `$width`, `$height`, `$image_type`.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `exif_thumbnail()`](../../../php/builtins/image/exif_thumbnail.md)
