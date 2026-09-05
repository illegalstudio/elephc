---
title: "image_type_to_mime_type() — internals"
description: "Compiler internals for image_type_to_mime_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 454
---

## `image_type_to_mime_type()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3666](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3666) (`image_type_to_mime_type`)
- **Function symbol**: `image_type_to_mime_type()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function image_type_to_mime_type(int $image_type): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `image_type_to_mime_type()`](../../../php/builtins/image/image_type_to_mime_type.md)
