---
title: "shell_exec() — internals"
description: "Compiler internals for shell_exec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 328
---

## `shell_exec()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/shell_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/shell_exec.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:698](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L698) (`lower_shell_exec`)
- **Function symbol**: `lower_shell_exec()`


### Lowering notes

- Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function shell_exec(string $command): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/shell_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/shell_exec.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `shell_exec()`](../../../php/builtins/process/shell_exec.md)
