---
title: "hash_update() — internals"
description: "Compiler internals for hash_update(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 445
---

## `hash_update()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/hash_prelude.rs`:136](https://github.com/illegalstudio/elephc/blob/main/src/hash_prelude.rs#L136) (`hash_update`)
- **Function symbol**: `hash_update()`


### Lowering notes

- Implemented by the compiler-injected hash prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function hash_update(HashContext $context, string $data): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_update()`](../../../php/builtins/string/hash_update.md)
