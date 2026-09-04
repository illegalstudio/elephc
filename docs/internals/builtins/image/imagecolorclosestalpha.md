---
title: "imagecolorclosestalpha() — internals"
description: "Compiler internals for imagecolorclosestalpha(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 467
---

## `imagecolorclosestalpha()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2418](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2418) (`imagecolorclosestalpha`)
- **Function symbol**: `imagecolorclosestalpha()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolorclosestalpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolorclosestalpha()`](../../../php/builtins/image/imagecolorclosestalpha.md)
