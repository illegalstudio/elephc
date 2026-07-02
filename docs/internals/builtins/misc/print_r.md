---
title: "print_r() — internals"
description: "Compiler internals for print_r(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 280
---

## `print_r()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/print_r.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/print_r.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/debug.rs`:24](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/debug.rs#L24) (`lower_print_r`)
- **Function symbol**: `lower_print_r()`


### Lowering notes

- Lowers `print_r(value)` for concrete scalar/resource values and array/hash shells.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function print_r(mixed $value): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `print_r()`](../../../php/builtins/misc/print_r.md)

