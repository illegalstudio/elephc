---
title: "fputcsv() — internals"
description: "Compiler internals for fputcsv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 166
---

## `fputcsv()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3005](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3005) (`lower_fputcsv`)
- **Function symbol**: `lower_fputcsv()`


### Lowering notes

- Lowers `fputcsv(stream, fields, separator?, enclosure?)` for string arrays.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fputcsv`

## Signature summary

```php
function fputcsv(resource $stream, array $fields, string $separator = ',', string $enclosure = '"', string $escape = '\\', string $eol = '\n'): int
```

## What the type checker enforces

- **Arity**: takes 2–6 arguments (4 optional).

## Cross-references

- [User reference for `fputcsv()`](../../../php/builtins/io/fputcsv.md)

