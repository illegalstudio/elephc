---
title: "is_finite() — internals"
description: "Compiler internals for is_finite(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 234
---

## `is_finite()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:169](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L169) (`lower_is_finite`)
- **Function symbol**: `lower_is_finite()`


### Lowering notes

- Lowers `is_finite()` by rejecting NaN and both infinities.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_finite(float $num): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_finite()`](../../../php/builtins/math/is_finite.md)

