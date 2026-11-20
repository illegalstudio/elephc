---
title: "filter_input() - internals"
description: "Compiler internals for filter_input(): lowering path, type checks, and runtime helpers."
---

## `filter_input()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_filter.rs)
- **Lowering**: [`src/filter_prelude/input.rs`](https://github.com/illegalstudio/elephc/blob/main/src/filter_prelude/input.rs) (`filter_input`)
- **Function symbol**: `filter_input()`


### Lowering notes

- Implemented by the compiler-injected filter prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function filter_input(int $type, string $var_name, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filter/filter_input.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filter/filter_input.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filter_input()`](../../../php/builtins/filter/filter_input.md)
