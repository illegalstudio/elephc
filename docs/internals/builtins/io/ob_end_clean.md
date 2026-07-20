---
title: "ob_end_clean() — internals"
description: "Compiler internals for ob_end_clean(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 190
---

## `ob_end_clean()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_end_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_end_clean.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:440](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L440) (`lower_ob_end_clean`)
- **Function symbol**: `lower_ob_end_clean()`


### Lowering notes

- Lowers `ob_end_clean()` to the discard-and-pop helper (bool result).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_end_clean`
- `__rt_ob_end_flush`
- `__rt_ob_flush`

## Signature summary

```php
function ob_end_clean(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_end_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_end_clean.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_end_clean()`](../../../php/builtins/io/ob_end_clean.md)
