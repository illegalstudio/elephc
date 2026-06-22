---
title: "popen() — internals"
description: "Compiler internals for popen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 293
---

## `popen()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3598](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3598) (`lower_popen`)
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

## Cross-references

- [User reference for `popen()`](../../../php/builtins/process/popen.md)

