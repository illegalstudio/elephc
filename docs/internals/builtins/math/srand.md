---
title: "srand() - internals"
description: "Compiler internals for srand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 615
---

## `srand()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/random_prelude.rs`:181](https://github.com/illegalstudio/elephc/blob/main/src/random_prelude.rs#L181) (`srand`)
- **Function symbol**: `srand()`


### Lowering notes

- Implemented by the compiler-injected random prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function srand(?int $seed = null, int $mode = 0): void
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `srand()`](../../../php/builtins/math/srand.md)
