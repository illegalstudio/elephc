---
title: "intdiv() — internals"
description: "Compiler internals for intdiv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 233
---

## `intdiv()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/binary.rs`:21](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/binary.rs#L21) (`lower_intdiv`)
- **Function symbol**: `lower_intdiv()`


### Lowering notes

- Lowers `intdiv()` for concrete integer-like numeric operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function intdiv(int $num1, int $num2): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `intdiv()`](../../../php/builtins/math/intdiv.md)

