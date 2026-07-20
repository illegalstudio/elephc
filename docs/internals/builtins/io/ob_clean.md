---
title: "ob_clean() — internals"
description: "Compiler internals for ob_clean(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 189
---

## `ob_clean()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_clean.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:435](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L435) (`lower_ob_clean`)
- **Function symbol**: `lower_ob_clean()`


### Lowering notes

- Lowers `ob_clean()` to the truncate-top-buffer helper (bool result).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_clean`
- `__rt_ob_end_clean`
- `__rt_ob_end_flush`
- `__rt_ob_flush`

## Signature summary

```php
function ob_clean(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_clean.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_clean()`](../../../php/builtins/io/ob_clean.md)
