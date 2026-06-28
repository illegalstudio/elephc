---
title: "getprotobyname() — internals"
description: "Compiler internals for getprotobyname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 163
---

## `getprotobyname()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3223](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3223) (`lower_getprotobyname`)
- **Function symbol**: `lower_getprotobyname()`


### Lowering notes

- Lowers `getprotobyname(protocol)` and boxes a missing entry as PHP `false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getprotobyname`

## Signature summary

```php
function getprotobyname(string $protocol): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `getprotobyname()`](../../../php/builtins/io/getprotobyname.md)

