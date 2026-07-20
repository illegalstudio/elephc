---
title: "preg_replace_callback() — internals"
description: "Compiler internals for preg_replace_callback(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 336
---

## `preg_replace_callback()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/preg_replace_callback.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/preg_replace_callback.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/regex.rs`:101](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/regex.rs#L101) (`lower_preg_replace_callback`)
- **Function symbol**: `lower_preg_replace_callback()`


### Lowering notes

- Lowers `preg_replace_callback(pattern, callback, subject)` through supported direct callbacks.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_replace_callback`

## Signature summary

```php
function preg_replace_callback(string $pattern, callable $callback, string $subject): string
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `preg_replace_callback()`](../../../php/builtins/regex/preg_replace_callback.md)
