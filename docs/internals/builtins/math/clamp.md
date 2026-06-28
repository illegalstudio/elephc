---
title: "clamp() — internals"
description: "Compiler internals for clamp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 224
---

## `clamp()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:80](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L80) (`lower_clamp`)
- **Function symbol**: `lower_clamp()`


### Lowering notes

- Lowers numeric `clamp(value, min, max)` calls with PHP-compatible bound checks.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function clamp(int $value, int $min, int $max): string
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `clamp()`](../../../php/builtins/math/clamp.md)

