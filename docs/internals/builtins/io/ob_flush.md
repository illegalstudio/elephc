---
title: "ob_flush() — internals"
description: "Compiler internals for ob_flush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 192
---

## `ob_flush()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_flush.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:456](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L456) (`lower_ob_flush`)
- **Function symbol**: `lower_ob_flush()`


### Lowering notes

- Lowers `ob_flush()` to the flush-keep-buffer helper (bool result).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_flush`

## Signature summary

```php
function ob_flush(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_flush.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_flush()`](../../../php/builtins/io/ob_flush.md)
