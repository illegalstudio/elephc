---
title: "mb_strlen() — internals"
description: "Compiler internals for mb_strlen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 375
---

## `mb_strlen()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_strlen.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:375](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L375) (`lower_mb_strlen`)
- **Function symbol**: `lower_mb_strlen()`


### Lowering notes

- Lowers `mb_strlen(string, encoding = null)` through the multibyte runtime helper.
- Omitted/null encodings use a null pointer plus zero length; explicit names stay byte strings for PHP-compatible case-insensitive lookup and `ValueError` handling.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mb_strlen`

## Signature summary

```php
function mb_strlen(string $string, string $encoding = null): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strlen.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `mb_strlen()`](../../../php/builtins/string/mb_strlen.md)
