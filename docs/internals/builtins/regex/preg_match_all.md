---
title: "preg_match_all() — internals"
description: "Compiler internals for preg_match_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 300
---

## `preg_match_all()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/regex.rs`:52](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/regex.rs#L52) (`lower_preg_match_all`)
- **Function symbol**: `lower_preg_match_all()`


### Lowering notes

- Lowers `preg_match_all(pattern, subject)` through the shared regex runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_match_all`

## Signature summary

```php
function preg_match_all(string $pattern, string $subject, array $matches): int
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.
- **By-reference parameters**: `$matches`.

## Cross-references

- [User reference for `preg_match_all()`](../../../php/builtins/regex/preg_match_all.md)

