---
title: "mb_ereg_match() — internals"
description: "Compiler internals for mb_ereg_match(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 315
---

## `mb_ereg_match()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_ereg_match.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_ereg_match.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/regex.rs`:52](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/regex.rs#L52) (`lower_mb_ereg_match`)
- **Function symbol**: `lower_mb_ereg_match()`


### Lowering notes

- Lowers `mb_ereg_match(pattern, subject, options = null)` as a start-anchored regex match.
- The bare delimiter-less pattern and subject use the shared regex string loader. Optional
- options are passed as a string pair when present, or as `(0, 0)` for `null`/omitted options.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mb_ereg_match`

## Signature summary

```php
function mb_ereg_match(string $pattern, string $subject, string $options = null): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `mb_ereg_match()`](../../../php/builtins/regex/mb_ereg_match.md)
