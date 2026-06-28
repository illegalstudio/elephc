---
title: "str_contains() — internals"
description: "Compiler internals for str_contains(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 363
---

## `str_contains()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:682](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L682) (`lower_str_contains`)
- **Function symbol**: `lower_str_contains()`


### Lowering notes

- Lowers `str_contains()` through `strpos()` and converts found positions to bool.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strpos`

## Signature summary

```php
function str_contains(string $haystack, string $needle): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `str_contains()`](../../../php/builtins/string/str_contains.md)

