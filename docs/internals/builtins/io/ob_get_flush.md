---
title: "ob_get_flush() — internals"
description: "Compiler internals for ob_get_flush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 195
---

## `ob_get_flush()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_flush.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:402](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L402) (`lower_ob_get_flush`)
- **Function symbol**: `lower_ob_get_flush()`


### Lowering notes

- Lowers `ob_get_flush()` through the composite runtime helper: REMOVABLE
- gating, handler FINAL phase, parent-sink flush, pop, and the raw contents.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_get_flush_pop`
- `__rt_ob_length`
- `__rt_ob_level`

## Signature summary

```php
function ob_get_flush(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_flush.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_flush()`](../../../php/builtins/io/ob_get_flush.md)
