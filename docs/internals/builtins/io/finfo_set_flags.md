---
title: "finfo_set_flags() - internals"
description: "Compiler internals for finfo_set_flags(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 325
---

## `finfo_set_flags()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/fileinfo_prelude/build.rs`:160](https://github.com/illegalstudio/elephc/blob/main/src/fileinfo_prelude/build.rs#L160) (`finfo_set_flags`)
- **Function symbol**: `finfo_set_flags()`


### Lowering notes

- Implemented by the compiler-injected fileinfo prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function finfo_set_flags(finfo $finfo, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_set_flags.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_set_flags.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `finfo_set_flags()`](../../../php/builtins/io/finfo_set_flags.md)
