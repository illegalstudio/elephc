---
title: "substr_replace() — internals"
description: "Compiler internals for substr_replace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 419
---

## `substr_replace()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/substr_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/substr_replace.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:792](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L792) (`lower_substr_replace`)
- **Function symbol**: `lower_substr_replace()`


### Lowering notes

- Lowers `substr_replace(string, replacement, start, length?)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_repeat`
- `__rt_substr_replace`

## Signature summary

```php
function substr_replace(string $string, string $replace, int $offset, int $length = null): string
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/substr_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/substr_replace.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `substr_replace()`](../../../php/builtins/string/substr_replace.md)
