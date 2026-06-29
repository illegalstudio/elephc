---
title: "str_split() — internals"
description: "Compiler internals for str_split(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 371
---

## `str_split()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:176](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L176) (`lower_str_split`)
- **Function symbol**: `lower_str_split()`


### Lowering notes

- Lowers `str_split(string, length?)` into the fixed-width string-array splitter.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_split`

## Signature summary

```php
function str_split(string $string, int $length): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `str_split()`](../../../php/builtins/string/str_split.md)

