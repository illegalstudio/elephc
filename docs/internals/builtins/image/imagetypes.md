---
title: "imagetypes() — internals"
description: "Compiler internals for imagetypes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 535
---

## `imagetypes()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3646](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3646) (`imagetypes`)
- **Function symbol**: `imagetypes()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagetypes(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagetypes()`](../../../php/builtins/image/imagetypes.md)
