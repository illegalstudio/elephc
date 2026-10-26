---
title: "random_bytes() - internals"
description: "Compiler internals for random_bytes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 601
---

## `random_bytes()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/random_prelude.rs`:316](https://github.com/illegalstudio/elephc/blob/main/src/random_prelude.rs#L316) (`random_bytes`)
- **Function symbol**: `random_bytes()`


### Lowering notes

- Implemented by the compiler-injected random prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function random_bytes(int $length): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `random_bytes()`](../../../php/builtins/math/random_bytes.md)
