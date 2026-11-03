---
title: "password_algos() - internals"
description: "Compiler internals for password_algos(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1021
---

## `password_algos()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/password_prelude.rs`:224](https://github.com/illegalstudio/elephc/blob/main/src/password_prelude.rs#L224) (`password_algos`)
- **Function symbol**: `password_algos()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_algos(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/password_algos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_algos.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `password_algos()`](../../../php/builtins/string/password_algos.md)
