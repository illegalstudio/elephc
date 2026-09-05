---
title: "gd_info() — internals"
description: "Compiler internals for gd_info(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 450
---

## `gd_info()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3656](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3656) (`gd_info`)
- **Function symbol**: `gd_info()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gd_info(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gd_info()`](../../../php/builtins/image/gd_info.md)
