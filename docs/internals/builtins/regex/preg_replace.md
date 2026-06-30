---
title: "preg_replace() — internals"
description: "Compiler internals for preg_replace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 316
---

## `preg_replace()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/regex.rs`:65](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/regex.rs#L65) (`lower_preg_replace`)
- **Function symbol**: `lower_preg_replace()`


### Lowering notes

- Lowers `preg_replace(pattern, replacement, subject)` through the regex replacement helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_replace`

## Signature summary

```php
function preg_replace(string $pattern, string $replacement, string $subject, int $limit = -1, int $count = null): string
```

## What the type checker enforces

- **Arity**: takes 3–5 arguments (2 optional).
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `preg_replace()`](../../../php/builtins/regex/preg_replace.md)

