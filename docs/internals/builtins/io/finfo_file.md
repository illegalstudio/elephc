---
title: "finfo_file() - internals"
description: "Compiler internals for finfo_file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 325
---

## `finfo_file()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/fileinfo_prelude/build.rs`:246](https://github.com/illegalstudio/elephc/blob/main/src/fileinfo_prelude/build.rs#L246) (`finfo_file`)
- **Function symbol**: `finfo_file()`


### Lowering notes

- Implemented by the compiler-injected fileinfo prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function finfo_file(finfo $finfo, string $filename, int $flags = 0, ?resource $context = null): string|false
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_file.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `finfo_file()`](../../../php/builtins/io/finfo_file.md)
