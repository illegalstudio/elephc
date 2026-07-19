---
title: "popen() — internals"
description: "Compiler internals for popen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 313
---

## `popen()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/popen.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/popen.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3603](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3603) (`lower_popen`)
- **Function symbol**: `lower_popen()`


### Lowering notes

- Lowers `popen(command, mode)` and boxes the process pipe as `resource|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_popen`

## Signature summary

```php
function popen(string $command, string $mode): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/popen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/popen.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `popen()`](../../../php/builtins/process/popen.md)
