---
title: "ob_start() — internals"
description: "Compiler internals for ob_start(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 201
---

## `ob_start()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_start.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_start.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:44](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L44) (`lower_ob_start`)
- **Function symbol**: `lower_ob_start()`


### Lowering notes

- Lowers `ob_start([$callback[, $chunk_size[, $flags]]])` to `__rt_ob_start_ex`.
- Resolves the handler triple (invocation stub, env word, display name) from
- the callback operand: `null` selects the default handler; a `Callable`
- descriptor is retained and invoked through `__rt_ob_invoke_descriptor`; a
- runtime string dispatches through the shared callable descriptor cases (a
- miss raises PHP's invalid-callback warning and returns `false`); a boxed
- `Mixed` value unboxes to one of those shapes at run time.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ob_invoke_descriptor`
- `__rt_ob_start_ex`

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
