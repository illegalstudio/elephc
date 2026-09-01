---
title: "exit() — internals"
description: "Compiler internals for exit(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 363
---

## `exit()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


### Lowering notes

- Lowered through the compiler's dedicated language-construct path.

## Semantic descriptor

Shared contract with a dedicated compiler language-construct implementation.

## EIR and runtime boundary

_Lowered by a dedicated compiler language-construct path._

## Signature summary

```php
function exit(int $status = 0): void
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/exit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/exit.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `exit()`](../../../php/builtins/process/exit.md)
