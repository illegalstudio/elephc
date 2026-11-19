---
title: "zip_entry_open() - internals"
description: "Compiler internals for zip_entry_open(): lowering path, type checks, and runtime helpers."
---

## `zip_entry_open()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_zip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_zip.rs)
- **Lowering**: [`src/zip_prelude/build.rs`](https://github.com/illegalstudio/elephc/blob/main/src/zip_prelude/build.rs) (`zip_entry_open`)
- **Function symbol**: `zip_entry_open()`


### Lowering notes

- Implemented by the compiler-injected zip prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function zip_entry_open(mixed $zip_dp, mixed $zip_entry, string $mode = 'rb'): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zip_entry_open()`](../../../php/builtins/io/zip_entry_open.md)
