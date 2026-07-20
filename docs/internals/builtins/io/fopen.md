---
title: "fopen() — internals"
description: "Compiler internals for fopen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 169
---

## `fopen()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fopen.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fopen.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:338](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L338) (`lower_fopen`)
- **Function symbol**: `lower_fopen()`


### Lowering notes

- Lowers `fopen(filename, mode)` and boxes stream resources or PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_tmpfile`

## Signature summary

```php
function fopen(string $filename, string $mode, bool $use_include_path = false, mixed $context = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fopen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fopen.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fopen()`](../../../php/builtins/io/fopen.md)
