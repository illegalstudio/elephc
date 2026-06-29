---
title: "number_format() — internals"
description: "Compiler internals for number_format(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 356
---

## `number_format()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:875](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L875) (`lower_number_format`)
- **Function symbol**: `lower_number_format()`


### Lowering notes

- Lowers `number_format()` by arranging its runtime helper arguments.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_number_format`

## Signature summary

```php
function number_format(float $num, int $decimals, string $decimal_separator, string $thousands_separator): string
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Cross-references

- [User reference for `number_format()`](../../../php/builtins/string/number_format.md)

