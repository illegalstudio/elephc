---
title: "str_split() — internals"
description: "Compiler internals for str_split(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 392
---

## `str_split()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/str_split.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_split.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:194](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L194) (`lower_str_split`)
- **Function symbol**: `lower_str_split()`


### Lowering notes

- Lowers `str_split(string, length?)` into the fixed-width string-array splitter.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_split`

## Signature summary

```php
function str_split(string $string, int $length = 1): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/str_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_split.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `str_split()`](../../../php/builtins/string/str_split.md)
