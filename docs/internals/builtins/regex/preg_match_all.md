---
title: "preg_match_all() — internals"
description: "Compiler internals for preg_match_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 316
---

## `preg_match_all()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/preg_match_all.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/preg_match_all.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/regex.rs`:49](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/regex.rs#L49) (`lower_preg_match_all`)
- **Function symbol**: `lower_preg_match_all()`


### Lowering notes

- Lowers `preg_match_all(pattern, subject)` through the shared regex runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_match_all`
- `__rt_preg_replace`

## Signature summary

```php
function preg_match_all(string $pattern, string $subject): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `preg_match_all()`](../../../php/builtins/regex/preg_match_all.md)
