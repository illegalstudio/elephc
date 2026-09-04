---
title: "buffer_new() — internals"
description: "Compiler internals for buffer_new(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 394
---

## `buffer_new()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


### Lowering notes

- Lowered through a dedicated AST/EIR syntax node.

## Semantic descriptor

Shared contract lowered through dedicated compiler syntax.

## EIR and runtime boundary

_Lowered through a dedicated AST/EIR syntax node._

## Signature summary

```php
function buffer_new(int $length): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_new.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_new.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `buffer_new()`](../../../php/builtins/pointer/buffer_new.md)
