---
title: "var_export() - internals"
description: "Compiler internals for var_export(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 423
---

## `var_export()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/var_export_prelude.rs`:374](https://github.com/illegalstudio/elephc/blob/main/src/var_export_prelude.rs#L374) (`var_export`)
- **Function symbol**: `var_export()`


### Lowering notes

- Implemented by the compiler-injected var_export prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function var_export(mixed $value, bool $return = false): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/var_export.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/var_export.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `var_export()`](../../../php/builtins/io/var_export.md)
