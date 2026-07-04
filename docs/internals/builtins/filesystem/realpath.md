---
title: "realpath() — internals"
description: "Compiler internals for realpath(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 139
---

## `realpath()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/realpath.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/realpath.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3690](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3690) (`lower_realpath`)
- **Function symbol**: `lower_realpath()`


### Lowering notes

- Lowers `realpath(path)` and boxes the owned runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_realpath`

## Signature summary

```php
function realpath(string $path): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `realpath()`](../../../php/builtins/filesystem/realpath.md)

