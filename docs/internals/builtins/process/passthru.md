---
title: "passthru() — internals"
description: "Compiler internals for passthru(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 306
---

## `passthru()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:714](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L714) (`lower_passthru`)
- **Function symbol**: `lower_passthru()`


### Lowering notes

- Lowers `passthru(command)` through libc `system()` for direct stdout passthrough.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_cstr`
- `__rt_shell_exec`

## Signature summary

```php
function passthru(string $command, int $result_code): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$result_code`.

## Cross-references

- [User reference for `passthru()`](../../../php/builtins/process/passthru.md)

