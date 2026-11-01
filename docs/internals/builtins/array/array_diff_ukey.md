---
title: "array_diff_ukey() - internals"
description: "Compiler internals for array_diff_ukey(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 12
---

## `array_diff_ukey()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/array_ucompare_prelude.rs`:48](https://github.com/illegalstudio/elephc/blob/main/src/array_ucompare_prelude.rs#L48) (`array_diff_ukey`)
- **Function symbol**: `array_diff_ukey()`


### Lowering notes

- Implemented by the compiler-injected array-ucompare prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function array_diff_ukey(array $array, ...$rest): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$rest`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$rest`.

## Cross-references

- [User reference for `array_diff_ukey()`](../../../php/builtins/array/array_diff_ukey.md)
