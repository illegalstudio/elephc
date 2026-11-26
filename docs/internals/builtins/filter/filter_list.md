---
title: "filter_list() - internals"
description: "Compiler internals for filter_list(): lowering path, type checks, and runtime helpers."
---

## `filter_list()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_filter.rs)
- **Lowering**: [`src/filter_prelude/surface.rs`](https://github.com/illegalstudio/elephc/blob/main/src/filter_prelude/surface.rs) (`filter_list`)
- **Function symbol**: `filter_list()`


### Lowering notes

- Implemented by the compiler-injected filter prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function filter_list(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filter/filter_list.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filter/filter_list.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filter_list()`](../../../php/builtins/filter/filter_list.md)
