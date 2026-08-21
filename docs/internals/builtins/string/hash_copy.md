---
title: "hash_copy() — internals"
description: "Compiler internals for hash_copy(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 418
---

## `hash_copy()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/hash_prelude.rs`:1](https://github.com/illegalstudio/elephc/blob/main/src/hash_prelude.rs#L1) (`hash_copy`)
- **Function symbol**: `hash_copy()`


### Lowering notes

- Implemented by a compiler-injected PHP prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function hash_copy(HashContext $context): HashContext
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_copy()`](../../../php/builtins/string/hash_copy.md)
