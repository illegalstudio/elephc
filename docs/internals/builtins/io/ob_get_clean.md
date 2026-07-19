---
title: "ob_get_clean() — internals"
description: "Compiler internals for ob_get_clean(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 193
---

## `ob_get_clean()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_clean.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:390](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L390) (`lower_ob_get_clean`)
- **Function symbol**: `lower_ob_get_clean()`


### Lowering notes

- Lowers `ob_get_clean()` through the composite runtime helper: REMOVABLE
- gating, handler CLEAN|FINAL phase, pop, and the raw contents (or `false`).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_get_clean_pop`
- `__rt_ob_get_flush_pop`
- `__rt_ob_length`

## Signature summary

```php
function ob_get_clean(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_clean.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_clean()`](../../../php/builtins/io/ob_get_clean.md)
