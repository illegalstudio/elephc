---
title: "mime_content_type() - internals"
description: "Compiler internals for mime_content_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 347
---

## `mime_content_type()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/fileinfo_prelude/build.rs`:172](https://github.com/illegalstudio/elephc/blob/main/src/fileinfo_prelude/build.rs#L172) (`mime_content_type`)
- **Function symbol**: `mime_content_type()`


### Lowering notes

- Implemented by the compiler-injected fileinfo prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mime_content_type(string|resource $filename): string|false
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/fileinfo/mime_content_type.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/mime_content_type.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `mime_content_type()`](../../../php/builtins/io/mime_content_type.md)
