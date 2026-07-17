---
title: "number_format() — internals"
description: "Compiler internals for number_format(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 378
---

## `number_format()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/number_format.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/number_format.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:938](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L938) (`lower_number_format`)
- **Function symbol**: `lower_number_format()`


### Lowering notes

- Lowers `number_format()` by arranging its runtime helper arguments.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_number_format`

## Signature summary

```php
function number_format(float $num, int $decimals = 0, string $decimal_separator = '.', string $thousands_separator = ','): string
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/formatting/number_format.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/number_format.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `number_format()`](../../../php/builtins/string/number_format.md)
