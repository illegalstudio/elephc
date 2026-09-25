---
title: "imagegrabscreen() - internals"
description: "Compiler internals for imagegrabscreen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 517
---

## `imagegrabscreen()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2249](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2249) (`imagegrabscreen`)
- **Function symbol**: `imagegrabscreen()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

- **Target support**: `windows-x86_64`

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagegrabscreen(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagegrabscreen()`](../../../php/builtins/image/imagegrabscreen.md)
