---
title: "ob_get_status() — internals"
description: "Compiler internals for ob_get_status(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 198
---

## `ob_get_status()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_status.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:497](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L497) (`lower_ob_get_status`)
- **Function symbol**: `lower_ob_get_status()`


### Lowering notes

- Lowers `ob_get_status([$full_status])` through the status-hash runtime helper
- and boxes the hash pointer as a Mixed associative array.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_get_status`
- `__rt_ob_list_handlers`

## Signature summary

```php
function ob_get_status(bool $full_status = false): array
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_status.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_status()`](../../../php/builtins/io/ob_get_status.md)
