---
title: "strtolower() — internals"
description: "Compiler internals for strtolower(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 416
---

## `strtolower()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/strtolower.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strtolower.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:75](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L75) (`lower_unary_string_runtime`)
- **Function symbol**: `lower_unary_string_runtime()`


### Lowering notes

- Lowers a one-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_htmlspecialchars`

## Signature summary

```php
function strtolower(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/strtolower.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strtolower.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `strtolower()`](../../../php/builtins/string/strtolower.md)
