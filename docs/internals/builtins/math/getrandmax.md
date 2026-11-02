---
title: "getrandmax() - internals"
description: "Compiler internals for getrandmax(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 588
---

## `getrandmax()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/random_prelude.rs`:194](https://github.com/illegalstudio/elephc/blob/main/src/random_prelude.rs#L194) (`getrandmax`)
- **Function symbol**: `getrandmax()`


### Lowering notes

- Implemented by the compiler-injected random prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function getrandmax(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `getrandmax()`](../../../php/builtins/math/getrandmax.md)
