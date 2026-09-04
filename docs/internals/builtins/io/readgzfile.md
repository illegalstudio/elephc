---
title: "readgzfile() — internals"
description: "Compiler internals for readgzfile(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 231
---

## `readgzfile()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:268](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L268) (`readgzfile`)
- **Function symbol**: `readgzfile()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function readgzfile(string $filename, int $use_include_path = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `readgzfile()`](../../../php/builtins/io/readgzfile.md)
