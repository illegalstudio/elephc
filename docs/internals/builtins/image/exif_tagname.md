---
title: "exif_tagname() — internals"
description: "Compiler internals for exif_tagname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 448
---

## `exif_tagname()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3921](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3921) (`exif_tagname`)
- **Function symbol**: `exif_tagname()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function exif_tagname(int $index): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `exif_tagname()`](../../../php/builtins/image/exif_tagname.md)
