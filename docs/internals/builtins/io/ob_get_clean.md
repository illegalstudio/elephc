---
title: "ob_get_clean() — internals"
description: "Compiler internals for ob_get_clean(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 193
---

## `ob_get_clean()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_clean.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:53](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L53) (`lower_ob_get_clean`)
- **Function symbol**: `lower_ob_get_clean()`


### Lowering notes

- Lowers `ob_get_clean()`: capture the top buffer's contents, then discard the
- buffer, returning the captured string (or `false` when no buffer is active).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_contents`
- `__rt_ob_end_clean`
- `__rt_ob_end_flush`

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
