---
title: "ob_implicit_flush() — internals"
description: "Compiler internals for ob_implicit_flush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 199
---

## `ob_implicit_flush()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_implicit_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_implicit_flush.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/output_buffering.rs`:474](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/output_buffering.rs#L474) (`lower_ob_implicit_flush`)
- **Function symbol**: `lower_ob_implicit_flush()`


### Lowering notes

- Lowers `ob_implicit_flush([$enable])`: store the flag (semantically inert in
- elephc — terminal writes are unbuffered syscalls) and return `true` like PHP 8.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ob_implicit_flush(bool $enable = true): bool
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_implicit_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_implicit_flush.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_implicit_flush()`](../../../php/builtins/io/ob_implicit_flush.md)
