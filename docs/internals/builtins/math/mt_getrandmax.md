---
title: "mt_getrandmax() - internals"
description: "Compiler internals for mt_getrandmax(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 601
---

## `mt_getrandmax()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/random_prelude.rs`:190](https://github.com/illegalstudio/elephc/blob/main/src/random_prelude.rs#L190) (`mt_getrandmax`)
- **Function symbol**: `mt_getrandmax()`


### Lowering notes

- Implemented by the compiler-injected random prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mt_getrandmax(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mt_getrandmax()`](../../../php/builtins/math/mt_getrandmax.md)
