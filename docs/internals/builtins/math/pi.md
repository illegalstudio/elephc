---
title: "pi() — internals"
description: "Compiler internals for pi(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 260
---

## `pi()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/pi.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/pi.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math.rs`:240](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math.rs#L240) (`lower_pi`)
- **Function symbol**: `lower_pi()`


### Lowering notes

- Lowers `pi()` as a data-section float constant.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function pi(): float
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `pi()`](../../../php/builtins/math/pi.md)
