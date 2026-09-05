---
title: "exif_read_data() — internals"
description: "Compiler internals for exif_read_data(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 447
---

## `exif_read_data()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3940](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3940) (`exif_read_data`)
- **Function symbol**: `exif_read_data()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function exif_read_data(string $filename, ?string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false): mixed
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `exif_read_data()`](../../../php/builtins/image/exif_read_data.md)
