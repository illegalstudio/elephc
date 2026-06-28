---
title: "tmpfile() — internals"
description: "Compiler internals for tmpfile(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 132
---

## `tmpfile()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4773](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4773) (`lower_tmpfile`)
- **Function symbol**: `lower_tmpfile()`


### Lowering notes

- Lowers `tmpfile()` and boxes the anonymous stream descriptor or PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filemtime`
- `__rt_linkinfo`
- `__rt_tmpfile`

## Signature summary

```php
function tmpfile(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `tmpfile()`](../../../php/builtins/filesystem/tmpfile.md)

