---
title: "ucfirst() — internals"
description: "Compiler internals for ucfirst(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 408
---

## `ucfirst()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/ucfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/ucfirst.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:114](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L114) (`lower_ucfirst`)
- **Function symbol**: `lower_ucfirst()`


### Lowering notes

- Lowers `ucfirst()` by copying the string and uppercasing the first ASCII byte.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strcopy`

## Signature summary

```php
function ucfirst(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/ucfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ucfirst.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ucfirst()`](../../../php/builtins/string/ucfirst.md)
