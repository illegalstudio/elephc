---
title: "empty() — internals"
description: "Compiler internals for empty(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 347
---

## `empty()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/count_empty.rs`:274](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/count_empty.rs#L274) (`lower_empty`)
- **Function symbol**: `lower_empty()`


### Lowering notes

- Lowers `empty()` for concrete scalar and array-like operands.

## Semantic descriptor

Shared contract with a dedicated compiler language-construct implementation.

## EIR and runtime boundary

- **Concrete helpers referenced directly by this lowering**:
  - `__rt_mixed_is_empty`

## Signature summary

```php
function empty(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/empty.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `empty()`](../../../php/builtins/misc/empty.md)
