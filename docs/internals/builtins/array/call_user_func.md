---
title: "call_user_func() — internals"
description: "Compiler internals for call_user_func(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 49
---

## `call_user_func()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/call_user_func.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/call_user_func.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:38](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L38) (`lower_call_user_func_builtin_escape`)
- **Function symbol**: `lower_call_user_func_builtin_escape()`


### Lowering notes

- Rejects `call_user_func*` calls that escaped the dedicated EIR callback lowering path.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_product`
- `__rt_array_product_float`
- `__rt_array_sum`
- `__rt_array_sum_float`

## Signature summary

```php
function call_user_func(callable $callback, ...$args): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$args`.

## Cross-references

- [User reference for `call_user_func()`](../../../php/builtins/array/call_user_func.md)
