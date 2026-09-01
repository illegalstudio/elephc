---
title: "hash_init() — internals"
description: "Compiler internals for hash_init(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 417
---

## `hash_init()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/hash_prelude.rs`:1](https://github.com/illegalstudio/elephc/blob/main/src/hash_prelude.rs#L1) (`hash_init`)
- **Function symbol**: `hash_init()`


### Lowering notes

- Implemented by the compiler-injected hash prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function hash_init(string $algo): HashContext
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_init.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_init()`](../../../php/builtins/string/hash_init.md)
