---
title: "filter_has_var() - internals"
description: "Compiler internals for filter_has_var(): lowering path, type checks, and runtime helpers."
---

## `filter_has_var()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_filter.rs)
- **Lowering**: [`src/filter_prelude/input.rs`](https://github.com/illegalstudio/elephc/blob/main/src/filter_prelude/input.rs) (`filter_has_var`)
- **Function symbol**: `filter_has_var()`


### Lowering notes

- Implemented by the compiler-injected filter prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function filter_has_var(int $input_type, string $var_name): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filter/filter_has_var.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filter/filter_has_var.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filter_has_var()`](../../../php/builtins/filter/filter_has_var.md)
