---
title: "time() — internals"
description: "Compiler internals for time(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 101
---

## `time()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/time.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/time.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:615](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L615) (`lower_time`)
- **Function symbol**: `lower_time()`


### Lowering notes

- Lowers `time()` through the shared wall-clock runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_exit`
- `__rt_stdout_write`
- `__rt_time`

## Signature summary

```php
function time(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/time/time.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/time.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `time()`](../../../php/builtins/date/time.md)
