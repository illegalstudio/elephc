---
title: "ob_start() — internals"
description: "Compiler internals for ob_start(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 201
---

## `ob_start()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_start.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_start.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:34](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L34) (`lower_ob_start`)
- **Function symbol**: `lower_ob_start()`


### Lowering notes

- Lowers `ob_start([$callback[, $chunk_size[, $flags]]])` to `__rt_ob_start`.
- The operands were already evaluated as separate EIR instructions (preserving
- side effects) and are intentionally unused: the checker only admits a `null`
- callback, and chunk size/flags are inert in elephc's buffer model.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_contents`
- `__rt_ob_end_clean`
- `__rt_ob_start`

## Signature summary

```php
function ob_start(mixed $callback = null, int $chunk_size = 0, int $flags = 112): bool
```

## What the type checker enforces

- **Arity**: takes 0–3 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_start.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_start.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_start()`](../../../php/builtins/io/ob_start.md)
