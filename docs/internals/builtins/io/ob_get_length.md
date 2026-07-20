---
title: "ob_get_length() — internals"
description: "Compiler internals for ob_get_length(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 196
---

## `ob_get_length()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_length.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_length.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:414](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L414) (`lower_ob_get_length`)
- **Function symbol**: `lower_ob_get_length()`


### Lowering notes

- Lowers `ob_get_length()` and boxes the length-or-false result (the runtime
- returns -1 when no buffer is active).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_clean`
- `__rt_ob_end_clean`
- `__rt_ob_length`
- `__rt_ob_level`

## Signature summary

```php
function ob_get_length(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_length.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_length.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_length()`](../../../php/builtins/io/ob_get_length.md)
