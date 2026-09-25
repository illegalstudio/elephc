---
title: "imagegrabwindow() - internals"
description: "Compiler internals for imagegrabwindow(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 518
---

## `imagegrabwindow()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2266](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2266) (`imagegrabwindow`)
- **Function symbol**: `imagegrabwindow()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

- **Target support**: `windows-x86_64`

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagegrabwindow(int $handle, bool $client_area = false): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagegrabwindow()`](../../../php/builtins/image/imagegrabwindow.md)
