---
title: "sscanf() — internals"
description: "Compiler internals for sscanf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 362
---

## `sscanf()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:163](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L163) (`lower_sscanf`)
- **Function symbol**: `lower_sscanf()`


### Lowering notes

- Lowers `sscanf(string, format)` into the shared scanner helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sscanf`
- `__rt_str_split`

## Signature summary

```php
function sscanf(string $string, string $format, ...$vars): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `sscanf()`](../../../php/builtins/string/sscanf.md)

