---
title: "str_pad() — internals"
description: "Compiler internals for str_pad(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 403
---

## `str_pad()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/str_pad.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_pad.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:881](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L881) (`lower_str_pad`)
- **Function symbol**: `lower_str_pad()`


### Lowering notes

- Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_pad`

## Signature summary

```php
function str_pad(string $string, int $length, string $pad_string = ' ', int $pad_type = 1): string
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/str_pad.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_pad.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `str_pad()`](../../../php/builtins/string/str_pad.md)
