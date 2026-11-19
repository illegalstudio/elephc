---
title: "zip_close() - internals"
description: "Compiler internals for zip_close(): lowering path, type checks, and runtime helpers."
---

## `zip_close()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_zip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_zip.rs)
- **Lowering**: [`src/zip_prelude/build.rs`](https://github.com/illegalstudio/elephc/blob/main/src/zip_prelude/build.rs) (`zip_close`)
- **Function symbol**: `zip_close()`


### Lowering notes

- Implemented by the compiler-injected zip prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function zip_close(mixed $zip): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zip_close()`](../../../php/builtins/io/zip_close.md)
