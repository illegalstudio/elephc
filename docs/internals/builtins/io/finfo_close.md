---
title: "finfo_close() - internals"
description: "Compiler internals for finfo_close(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 324
---

## `finfo_close()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/fileinfo_prelude/build.rs`:156](https://github.com/illegalstudio/elephc/blob/main/src/fileinfo_prelude/build.rs#L156) (`finfo_close`)
- **Function symbol**: `finfo_close()`


### Lowering notes

- Implemented by the compiler-injected fileinfo prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function finfo_close(finfo $finfo): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_close.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `finfo_close()`](../../../php/builtins/io/finfo_close.md)
