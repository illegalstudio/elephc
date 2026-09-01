---
title: "isset() — internals"
description: "Compiler internals for isset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 332
---

## `isset()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/isset.rs`:24](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/isset.rs#L24) (`lower_isset`)
- **Function symbol**: `lower_isset()`


### Lowering notes

- Lowers `isset()` for values already evaluated by the EIR frontend.

## Semantic descriptor

Shared contract with a dedicated compiler language-construct implementation.

## EIR and runtime boundary

_Lowered by a dedicated compiler language-construct path._

## Signature summary

```php
function isset(mixed $var, ...$vars): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$vars`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/isset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/isset.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `isset()`](../../../php/builtins/misc/isset.md)
