---
title: "imagesavealpha() — internals"
description: "Compiler internals for imagesavealpha(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 525
---

## `imagesavealpha()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2522](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2522) (`imagesavealpha`)
- **Function symbol**: `imagesavealpha()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagesavealpha(mixed $image, bool $enable): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagesavealpha()`](../../../php/builtins/image/imagesavealpha.md)
