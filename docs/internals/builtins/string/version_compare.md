---
title: "version_compare() - internals"
description: "Compiler internals for version_compare(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1089
---

## `version_compare()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/version_compare_prelude.rs`:444](https://github.com/illegalstudio/elephc/blob/main/src/version_compare_prelude.rs#L444) (`version_compare`)
- **Function symbol**: `version_compare()`


### Lowering notes

- Implemented by the compiler-injected version_compare prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function version_compare(string $version1, string $version2, string $operator = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/version_compare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/version_compare.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `version_compare()`](../../../php/builtins/string/version_compare.md)
