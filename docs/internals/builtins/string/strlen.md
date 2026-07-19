---
title: "strlen() — internals"
description: "Compiler internals for strlen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 411
---

## `strlen()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strlen.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1094](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1094) (`lower_strlen`)
- **Function symbol**: `lower_strlen()`


### Lowering notes

- Lowers `strlen()` by coercing string-like values and returning the byte length.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_string`

## Signature summary

```php
function strlen(string $string): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strlen.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `strlen()`](../../../php/builtins/string/strlen.md)
