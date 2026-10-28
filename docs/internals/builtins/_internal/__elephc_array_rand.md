---
title: "__elephc_array_rand() - internals"
description: "Compiler internals for __elephc_array_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1099
---

## `__elephc_array_rand()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/random_prelude.rs`:212](https://github.com/illegalstudio/elephc/blob/main/src/random_prelude.rs#L212) (`__elephc_array_rand`)
- **Function symbol**: `__elephc_array_rand()`


### Lowering notes

- Implemented by the compiler-injected random prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function __elephc_array_rand(array $array, int $num = 1): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
