---
title: "str_contains() — internals"
description: "Compiler internals for str_contains(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 400
---

## `str_contains()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/str_contains.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_contains.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:745](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L745) (`lower_str_contains`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/str_contains.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_contains.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `str_contains()`](../../../php/builtins/string/str_contains.md)
