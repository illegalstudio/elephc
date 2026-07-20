---
title: "preg_replace() — internals"
description: "Compiler internals for preg_replace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 335
---

## `preg_replace()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/preg_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/preg_replace.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/regex.rs`:79](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/regex.rs#L79) (`lower_preg_replace`)
- **Function symbol**: `lower_preg_replace()`


### Lowering notes

- Lowers `preg_replace(pattern, replacement, subject)` through the regex replacement helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_replace`

## Signature summary

```php
function preg_replace(string $pattern, string $replacement, string $subject): string
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `preg_replace()`](../../../php/builtins/regex/preg_replace.md)
