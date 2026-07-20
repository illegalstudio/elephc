---
title: "ob_list_handlers() — internals"
description: "Compiler internals for ob_list_handlers(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 200
---

## `ob_list_handlers()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_list_handlers.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_list_handlers.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:512](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L512) (`lower_ob_list_handlers`)
- **Function symbol**: `lower_ob_list_handlers()`


### Lowering notes

- Lowers `ob_list_handlers()` to the handler-name string-array helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_from_value`
- `__rt_ob_list_handlers`

## Signature summary

```php
function ob_list_handlers(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_list_handlers.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_list_handlers.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_list_handlers()`](../../../php/builtins/io/ob_list_handlers.md)
