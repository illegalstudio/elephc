---
title: "fstat() — internals"
description: "Compiler internals for fstat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 155
---

## `fstat()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5533](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5533) (`lower_fstat`)
- **Function symbol**: `lower_fstat()`


### Lowering notes

- Lowers `fstat(stream)` and boxes the runtime stat array or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fstat_array`

## Signature summary

```php
function fstat(resource $stream): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fstat()`](../../../php/builtins/io/fstat.md)

