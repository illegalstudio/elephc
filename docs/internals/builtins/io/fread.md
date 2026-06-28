---
title: "fread() — internals"
description: "Compiler internals for fread(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 152
---

## `fread()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2595](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2595) (`lower_fread`)
- **Function symbol**: `lower_fread()`


### Lowering notes

- Lowers `fread(stream, length)` using the shared runtime file-read helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fread`

## Signature summary

```php
function fread(resource $stream, int $length): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `fread()`](../../../php/builtins/io/fread.md)

