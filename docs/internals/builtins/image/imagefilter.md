---
title: "imagefilter() — internals"
description: "Compiler internals for imagefilter(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 505
---

## `imagefilter()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3263](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3263) (`imagefilter`)
- **Function symbol**: `imagefilter()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagefilter(mixed $image, int $filter, int $arg1 = 0, int $arg2 = 0, int $arg3 = 0, int $arg4 = 0): bool
```

## What the type checker enforces

- **Arity**: takes 2–6 arguments (4 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagefilter()`](../../../php/builtins/image/imagefilter.md)
