---
title: "gzencode() — internals"
description: "Compiler internals for gzencode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 463
---

## `gzencode()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:165](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L165) (`gzencode`)
- **Function symbol**: `gzencode()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzencode(string $data, int $level = -1, int $encoding = 31): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzencode()`](../../../php/builtins/string/gzencode.md)
