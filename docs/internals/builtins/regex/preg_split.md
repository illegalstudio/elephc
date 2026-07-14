---
title: "preg_split() — internals"
description: "Compiler internals for preg_split(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 324
---

## `preg_split()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/preg_split.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/preg_split.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/regex.rs`:405](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/regex.rs#L405) (`lower_preg_split`)
- **Function symbol**: `lower_preg_split()`


### Lowering notes

- Lowers `preg_split(pattern, subject, limit?, flags?)` through the regex split helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_split`

## Signature summary

```php
function preg_split(string $pattern, string $subject, int $limit = -1, int $flags = 0): array
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_split.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `preg_split()`](../../../php/builtins/regex/preg_split.md)
