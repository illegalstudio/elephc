---
title: "chown() — internals"
description: "Compiler internals for chown(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 85
---

## `chown()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3830](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3830) (`lower_chown`)
- **Function symbol**: `lower_chown()`


### Lowering notes

- Lowers `chown(path, owner)` for integer UIDs and string user names.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function chown(string $filename, int $user): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `chown()`](../../../php/builtins/filesystem/chown.md)

