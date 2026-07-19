---
title: "trim() — internals"
description: "Compiler internals for trim(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 420
---

## `trim()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/trim.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/trim.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:130](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L130) (`lower_trim_like`)
- **Function symbol**: `lower_trim_like()`


### Lowering notes

- Lowers `trim()`/`ltrim()`/`rtrim()`/`chop()` for default and explicit masks.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function trim(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/trim.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/trim.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `trim()`](../../../php/builtins/string/trim.md)
