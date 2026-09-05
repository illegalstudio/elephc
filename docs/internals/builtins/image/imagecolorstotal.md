---
title: "imagecolorstotal() — internals"
description: "Compiler internals for imagecolorstotal(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 477
---

## `imagecolorstotal()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2498](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2498) (`imagecolorstotal`)
- **Function symbol**: `imagecolorstotal()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolorstotal(mixed $image): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolorstotal()`](../../../php/builtins/image/imagecolorstotal.md)
