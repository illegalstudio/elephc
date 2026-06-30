---
title: "fileatime() — internals"
description: "Compiler internals for fileatime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 107
---

## `fileatime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5468](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5468) (`lower_fileatime`)
- **Function symbol**: `lower_fileatime()`


### Lowering notes

- Lowers `fileatime(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileatime`
- `__rt_filectime`
- `__rt_fileowner`
- `__rt_fileperms`

## Signature summary

```php
function fileatime(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fileatime()`](../../../php/builtins/filesystem/fileatime.md)

