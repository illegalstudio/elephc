---
title: "hash_final() — internals"
description: "Compiler internals for hash_final(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 415
---

## `hash_final()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/hash_prelude.rs`:1](https://github.com/illegalstudio/elephc/blob/main/src/hash_prelude.rs#L1) (`hash_final`)
- **Function symbol**: `hash_final()`


### Lowering notes

- Implemented by the compiler-injected hash prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function hash_final(HashContext $context, bool $binary = false): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_final.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_final.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_final()`](../../../php/builtins/string/hash_final.md)
