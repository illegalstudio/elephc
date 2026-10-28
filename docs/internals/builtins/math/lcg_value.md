---
title: "lcg_value() - internals"
description: "Compiler internals for lcg_value(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 595
---

## `lcg_value()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/random_prelude.rs`:198](https://github.com/illegalstudio/elephc/blob/main/src/random_prelude.rs#L198) (`lcg_value`)
- **Function symbol**: `lcg_value()`


### Lowering notes

- Implemented by the compiler-injected random prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function lcg_value(): float
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `lcg_value()`](../../../php/builtins/math/lcg_value.md)
