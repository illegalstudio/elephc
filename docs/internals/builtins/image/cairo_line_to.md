---
title: "cairo_line_to() — internals"
description: "Compiler internals for cairo_line_to(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 411
---

## `cairo_line_to()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13489](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13489) (`cairo_line_to`)
- **Function symbol**: `cairo_line_to()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_line_to(mixed $context, float $x, float $y): void
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_line_to()`](../../../php/builtins/image/cairo_line_to.md)
