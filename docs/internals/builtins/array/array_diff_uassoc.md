---
title: "array_diff_uassoc() - internals"
description: "Compiler internals for array_diff_uassoc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 11
---

## `array_diff_uassoc()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/array_ucompare_prelude.rs`:47](https://github.com/illegalstudio/elephc/blob/main/src/array_ucompare_prelude.rs#L47) (`array_diff_uassoc`)
- **Function symbol**: `array_diff_uassoc()`


### Lowering notes

- Implemented by the compiler-injected array-ucompare prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function array_diff_uassoc(array $array, ...$rest): array
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

- [User reference for `array_diff_uassoc()`](../../../php/builtins/array/array_diff_uassoc.md)
