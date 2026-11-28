---
title: "zip_entry_read() - internals"
description: "Compiler internals for zip_entry_read(): lowering path, type checks, and runtime helpers."
---

## `zip_entry_read()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_zip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_zip.rs)
- **Lowering**: [`src/zip_prelude/build.rs`](https://github.com/illegalstudio/elephc/blob/main/src/zip_prelude/build.rs) (`zip_entry_read`)
- **Function symbol**: `zip_entry_read()`


### Lowering notes

- Implemented by the compiler-injected zip prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function zip_entry_read(mixed $zip_entry, int $len = 1024): string|false
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zip_entry_read()`](../../../php/builtins/io/zip_entry_read.md)
