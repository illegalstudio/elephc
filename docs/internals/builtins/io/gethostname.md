---
title: "gethostname() — internals"
description: "Compiler internals for gethostname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 162
---

## `gethostname()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3188](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3188) (`lower_gethostname`)
- **Function symbol**: `lower_gethostname()`


### Lowering notes

- Lowers `gethostname()` through the shared runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_gethostbyaddr`
- `__rt_gethostbyname`
- `__rt_gethostname`

## Signature summary

```php
function gethostname(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `gethostname()`](../../../php/builtins/io/gethostname.md)

