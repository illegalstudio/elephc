---
title: "lcfirst() — internals"
description: "Compiler internals for lcfirst(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 385
---

## `lcfirst()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/lcfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/lcfirst.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:121](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L121) (`lower_lcfirst`)
- **Function symbol**: `lower_lcfirst()`


### Lowering notes

- Lowers `lcfirst()` by copying the string and lowercasing the first ASCII byte.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strcopy`

## Signature summary

```php
function lcfirst(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/lcfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/lcfirst.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `lcfirst()`](../../../php/builtins/string/lcfirst.md)
