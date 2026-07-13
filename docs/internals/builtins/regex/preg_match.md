---
title: "preg_match() — internals"
description: "Compiler internals for preg_match(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 320
---

## `preg_match()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/preg_match.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/preg_match.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/regex.rs`:28](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/regex.rs#L28) (`lower_preg_match`)
- **Function symbol**: `lower_preg_match()`


### Lowering notes

- Lowers `preg_match(pattern, subject)` through the shared regex runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_match`
- `__rt_preg_match_capture`

## Signature summary

```php
function preg_match(string $pattern, string $subject, array $matches = []): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).
- **By-reference parameters**: `$matches`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_match.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_match.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$matches`.

## Cross-references

- [User reference for `preg_match()`](../../../php/builtins/regex/preg_match.md)
