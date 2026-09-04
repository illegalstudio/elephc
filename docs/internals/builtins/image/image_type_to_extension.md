---
title: "image_type_to_extension() — internals"
description: "Compiler internals for image_type_to_extension(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 453
---

## `image_type_to_extension()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3737](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3737) (`image_type_to_extension`)
- **Function symbol**: `image_type_to_extension()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function image_type_to_extension(int $image_type, bool $include_dot = true): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `image_type_to_extension()`](../../../php/builtins/image/image_type_to_extension.md)
