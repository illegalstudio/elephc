---
title: "system() — internals"
description: "Compiler internals for system(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 313
---

## `system()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/system.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/system.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:746](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L746) (`lower_system`)
- **Function symbol**: `lower_system()`


### Lowering notes

- Lowers `system(command)` through libc `system()` and returns the compiler's empty string result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_shell_exec`

## Signature summary

```php
function system(string $command): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `system()`](../../../php/builtins/process/system.md)
