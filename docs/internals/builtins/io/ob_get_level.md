---
title: "ob_get_level() — internals"
description: "Compiler internals for ob_get_level(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 197
---

## `ob_get_level()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_level.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_level.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L425) (`lower_ob_get_level`)
- **Function symbol**: `lower_ob_get_level()`


### Lowering notes

- Lowers `ob_get_level()` to the plain integer nesting-depth query.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_clean`
- `__rt_ob_end_clean`
- `__rt_ob_end_flush`
- `__rt_ob_level`

## Signature summary

```php
function ob_get_level(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_level.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_level.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_level()`](../../../php/builtins/io/ob_get_level.md)
