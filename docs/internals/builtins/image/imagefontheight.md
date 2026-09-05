---
title: "imagefontheight() — internals"
description: "Compiler internals for imagefontheight(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 507
---

## `imagefontheight()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2976](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2976) (`imagefontheight`)
- **Function symbol**: `imagefontheight()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagefontheight(int $font): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagefontheight()`](../../../php/builtins/image/imagefontheight.md)
