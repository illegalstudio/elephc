---
title: "array_find_key() - internals"
description: "Compiler internals for array_find_key(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 17
---

## `array_find_key()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/array_edge_prelude.rs`:49](https://github.com/illegalstudio/elephc/blob/main/src/array_edge_prelude.rs#L49) (`array_find_key`)
- **Function symbol**: `array_find_key()`


### Lowering notes

- Implemented by the compiler-injected array-edge prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function array_find_key(array $array, mixed $callback): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_find.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_find.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_find_key()`](../../../php/builtins/array/array_find_key.md)
