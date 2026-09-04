---
title: "_cairo_fx() — internals"
description: "Compiler internals for _cairo_fx(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 960
---

## `_cairo_fx()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:12421](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L12421) (`_cairo_fx`)
- **Function symbol**: `_cairo_fx()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function _cairo_fx(mixed $v): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
