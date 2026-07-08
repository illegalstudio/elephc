---
title: "print_r() — internals"
description: "Compiler internals for print_r(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 280
---

## `print_r()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/print_r.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/print_r.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/debug.rs`:30](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/debug.rs#L30) (`lower_print_r`)
- **Function symbol**: `lower_print_r()`


### Lowering notes

- Lowers `print_r(value)` for concrete scalar/resource values and array/hash shells.
- With one operand the value is rendered to stdout (PHP `print_r` echo mode) and
- the call returns `true`. With two operands where the second is a constant
- `true`, the value is rendered into the in-memory capture buffer and returned
- as an owned string via `__rt_pr_finish` (PHP `print_r($v, true)` return mode).
- A constant `false` (or any non-`true` second operand) keeps the echo-mode path.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_pr_finish`

## Signature summary

```php
function print_r(mixed $value, bool $return = false): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `print_r()`](../../../php/builtins/misc/print_r.md)
