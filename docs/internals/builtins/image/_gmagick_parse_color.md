---
title: "_gmagick_parse_color() — internals"
description: "Compiler internals for _gmagick_parse_color(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 964
---

## `_gmagick_parse_color()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:10149](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L10149) (`_gmagick_parse_color`)
- **Function symbol**: `_gmagick_parse_color()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function _gmagick_parse_color(string $c): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
