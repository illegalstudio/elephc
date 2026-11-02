---
title: "finfo_buffer() - internals"
description: "Compiler internals for finfo_buffer(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 323
---

## `finfo_buffer()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/fileinfo_prelude/build.rs`:163](https://github.com/illegalstudio/elephc/blob/main/src/fileinfo_prelude/build.rs#L163) (`finfo_buffer`)
- **Function symbol**: `finfo_buffer()`


### Lowering notes

- Implemented by the compiler-injected fileinfo prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function finfo_buffer(finfo $finfo, string $string, int $flags = 0, ?resource $context = null): string|false
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_buffer.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_buffer.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `finfo_buffer()`](../../../php/builtins/io/finfo_buffer.md)
