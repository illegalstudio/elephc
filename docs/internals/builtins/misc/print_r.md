---
title: "print_r() — internals"
description: "Compiler internals for print_r(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 280
---

## `print_r()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/print_r.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/print_r.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/debug.rs`:34](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/debug.rs#L34) (`lower_print_r`)
- **Function symbol**: `lower_print_r()`


### Lowering notes

- Lowers `print_r(value, $return = false)` for concrete scalar/resource values
- and array/hash shells.
- Dispatch follows the call's static result type, which the checker
- (`src/builtins/io/print_r.rs`) and the EIR return-type override
- (`print_r_builtin_return_type_for_args`) derive from the `$return` flag:
- - `Str` (literal `true`): render into the capture buffer and return the owned
- string finalized by `__rt_pr_finish`.
- - `Bool` (flag absent or literal `false`): render to stdout and return `true`.
- - `Mixed` (runtime flag): select the mode at runtime; see
- `lower_print_r_runtime_flag`.

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
