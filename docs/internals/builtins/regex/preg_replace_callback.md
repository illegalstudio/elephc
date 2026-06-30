---
title: "preg_replace_callback() — internals"
description: "Compiler internals for preg_replace_callback(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 317
---

## `preg_replace_callback()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/regex.rs`:90](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/regex.rs#L90) (`lower_preg_replace_callback`)
- **Function symbol**: `lower_preg_replace_callback()`


### Lowering notes

- Lowers `preg_replace_callback(pattern, callback, subject)` through supported direct callbacks.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_replace_callback`

## Signature summary

```php
function preg_replace_callback(string $pattern, callable $callback, string $subject, int $limit = -1, int $count = null, int $flags = 0): array
```

## What the type checker enforces

- **Arity**: takes 3–6 arguments (3 optional).
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `preg_replace_callback()`](../../../php/builtins/regex/preg_replace_callback.md)

