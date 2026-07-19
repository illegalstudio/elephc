---
title: "ob_get_contents() — internals"
description: "Compiler internals for ob_get_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 194
---

## `ob_get_contents()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_contents.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:378](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L378) (`lower_ob_get_contents`)
- **Function symbol**: `lower_ob_get_contents()`


### Lowering notes

- Lowers `ob_get_contents()` and boxes the runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_contents`
- `__rt_ob_get_clean_pop`
- `__rt_ob_get_flush_pop`

## Signature summary

```php
function ob_get_contents(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_contents.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_contents()`](../../../php/builtins/io/ob_get_contents.md)
