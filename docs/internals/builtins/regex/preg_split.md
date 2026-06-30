---
title: "preg_split() — internals"
description: "Compiler internals for preg_split(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 318
---

## `preg_split()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/regex.rs`:374](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/regex.rs#L374) (`lower_preg_split`)
- **Function symbol**: `lower_preg_split()`


### Lowering notes

- Lowers `preg_split(pattern, subject, limit?, flags?)` through the regex split helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_preg_split`

## Signature summary

```php
function preg_split(string $pattern, string $subject, int $limit, int $flags): array
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `preg_split()`](../../../php/builtins/regex/preg_split.md)

