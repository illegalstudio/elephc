---
title: "strlen() — internals"
description: "Compiler internals for strlen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 376
---

## `strlen()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:998](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L998) (`lower_strlen`)
- **Function symbol**: `lower_strlen()`


### Lowering notes

- Lowers `strlen()` by coercing string-like values and returning the byte length.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_string`

## Signature summary

```php
function strlen(string $string): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `strlen()`](../../../php/builtins/string/strlen.md)

