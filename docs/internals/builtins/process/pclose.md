---
title: "pclose() — internals"
description: "Compiler internals for pclose(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 307
---

## `pclose()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3630](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3630) (`lower_pclose`)
- **Function symbol**: `lower_pclose()`


### Lowering notes

- Lowers `pclose(handle)` and returns the child process status.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_pclose`

## Signature summary

```php
function pclose(resource $handle): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `pclose()`](../../../php/builtins/process/pclose.md)

