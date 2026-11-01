---
title: "array_change_key_case() - internals"
description: "Compiler internals for array_change_key_case(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 3
---

## `array_change_key_case()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/array_edge_prelude.rs`:77](https://github.com/illegalstudio/elephc/blob/main/src/array_edge_prelude.rs#L77) (`array_change_key_case`)
- **Function symbol**: `array_change_key_case()`


### Lowering notes

- Implemented by the compiler-injected array-edge prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function array_change_key_case(array $array, int $case = 0): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_change_key_case.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_change_key_case.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_change_key_case()`](../../../php/builtins/array/array_change_key_case.md)
