---
title: "log() — internals"
description: "Compiler internals for log(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 237
---

## `log()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/libm.rs`:51](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/libm.rs#L51) (`lower_log`)
- **Function symbol**: `lower_log()`


### Lowering notes

- Lowers `log()` in one-argument and base-changing two-argument forms.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function log(float $num, float $base): float
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `log()`](../../../php/builtins/math/log.md)

