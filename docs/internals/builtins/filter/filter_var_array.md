---
title: "filter_var_array() - internals"
description: "Compiler internals for filter_var_array(): lowering path, type checks, and runtime helpers."
---

## `filter_var_array()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_filter.rs)
- **Lowering**: [`src/filter_prelude/input.rs`](https://github.com/illegalstudio/elephc/blob/main/src/filter_prelude/input.rs) (`filter_var_array`)
- **Function symbol**: `filter_var_array()`


### Lowering notes

- Implemented by the compiler-injected filter prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function filter_var_array(array $array, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filter/filter_var_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filter/filter_var_array.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filter_var_array()`](../../../php/builtins/filter/filter_var_array.md)
