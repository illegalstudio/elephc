---
title: "password_get_info() - internals"
description: "Compiler internals for password_get_info(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1020
---

## `password_get_info()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/password_prelude.rs`:111](https://github.com/illegalstudio/elephc/blob/main/src/password_prelude.rs#L111) (`password_get_info`)
- **Function symbol**: `password_get_info()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_get_info(string $hash): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/password_get_info.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_get_info.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `password_get_info()`](../../../php/builtins/string/password_get_info.md)
