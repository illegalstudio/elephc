---
title: "_elephc_img_output() — internals"
description: "Compiler internals for _elephc_img_output(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 962
---

## `_elephc_img_output()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3544](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3544) (`_elephc_img_output`)
- **Function symbol**: `_elephc_img_output()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function _elephc_img_output(int $handle, int $fmt, string $file, int $quality): bool
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
