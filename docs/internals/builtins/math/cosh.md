---
title: "cosh() — internals"
description: "Compiler internals for cosh(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 226
---

## `cosh()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function cosh(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `cosh()`](../../../php/builtins/math/cosh.md)

