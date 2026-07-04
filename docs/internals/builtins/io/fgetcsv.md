---
title: "fgetcsv() — internals"
description: "Compiler internals for fgetcsv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 159
---

## `fgetcsv()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fgetcsv.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fgetcsv.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2993](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2993) (`lower_fgetcsv`)
- **Function symbol**: `lower_fgetcsv()`


### Lowering notes

- Lowers `fgetcsv(stream, separator?, enclosure?)` through the CSV row runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fgetcsv`
- `__rt_fputcsv`

## Signature summary

```php
function fgetcsv(resource $stream, int $length = null, string $separator = ','): array
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `fgetcsv()`](../../../php/builtins/io/fgetcsv.md)

