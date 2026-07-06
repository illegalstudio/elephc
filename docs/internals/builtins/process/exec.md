---
title: "exec() — internals"
description: "Compiler internals for exec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 301
---

## `exec()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/exec.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/exec.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:690](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L690) (`lower_exec`)
- **Function symbol**: `lower_exec()`


### Lowering notes

- Lowers `exec(command)` by capturing shell stdout through the shared runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function exec(string $command): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `exec()`](../../../php/builtins/process/exec.md)
