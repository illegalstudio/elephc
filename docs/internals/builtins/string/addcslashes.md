---
title: "addcslashes() - internals"
description: "Compiler internals for addcslashes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 895
---

## `addcslashes()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/addcslashes_prelude.rs`:131](https://github.com/illegalstudio/elephc/blob/main/src/addcslashes_prelude.rs#L131) (`addcslashes`)
- **Function symbol**: `addcslashes()`


### Lowering notes

- Implemented by the compiler-injected addcslashes prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function addcslashes(string $string, string $characters): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `addcslashes()`](../../../php/builtins/string/addcslashes.md)
