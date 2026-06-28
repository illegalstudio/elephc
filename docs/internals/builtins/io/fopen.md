---
title: "fopen() — internals"
description: "Compiler internals for fopen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 148
---

## `fopen()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:233](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L233) (`lower_fopen`)
- **Function symbol**: `lower_fopen()`


### Lowering notes

- Lowers `fopen(filename, mode)` and boxes stream resources or PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_tmpfile`

## Signature summary

```php
function fopen(string $filename, string $mode, bool $use_include_path, mixed $context): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `fopen()`](../../../php/builtins/io/fopen.md)

