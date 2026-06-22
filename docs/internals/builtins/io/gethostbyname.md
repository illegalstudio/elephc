---
title: "gethostbyname() — internals"
description: "Compiler internals for gethostbyname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 161
---

## `gethostbyname()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3417](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3417) (`lower_gethostbyname`)
- **Function symbol**: `lower_gethostbyname()`


### Lowering notes

- Lowers `gethostbyname(hostname)` through the shared runtime resolver.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_gethostbyaddr`
- `__rt_gethostbyname`

## Signature summary

```php
function gethostbyname(string $hostname): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `gethostbyname()`](../../../php/builtins/io/gethostbyname.md)

