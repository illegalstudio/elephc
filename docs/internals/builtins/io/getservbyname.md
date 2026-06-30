---
title: "getservbyname() — internals"
description: "Compiler internals for getservbyname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 180
---

## `getservbyname()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3486](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3486) (`lower_getservbyname`)
- **Function symbol**: `lower_getservbyname()`


### Lowering notes

- Lowers `getservbyname(service, protocol)` and boxes a missing entry as PHP `false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getservbyname`

## Signature summary

```php
function getservbyname(string $service, string $protocol): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `getservbyname()`](../../../php/builtins/io/getservbyname.md)

