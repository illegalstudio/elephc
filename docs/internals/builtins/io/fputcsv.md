---
title: "fputcsv() — internals"
description: "Compiler internals for fputcsv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 172
---

## `fputcsv()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fputcsv.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fputcsv.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3005](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3005) (`lower_fputcsv`)
- **Function symbol**: `lower_fputcsv()`


### Lowering notes

- Lowers `fputcsv(stream, fields, separator?, enclosure?)` for string arrays.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fputcsv`

## Signature summary

```php
function fputcsv(resource $stream, array $fields, string $separator = ',', string $enclosure = '"'): int
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fputcsv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fputcsv.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fputcsv()`](../../../php/builtins/io/fputcsv.md)
