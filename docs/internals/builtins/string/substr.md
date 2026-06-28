---
title: "substr() — internals"
description: "Compiler internals for substr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 381
---

## `substr()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:713](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L713) (`lower_substr`)
- **Function symbol**: `lower_substr()`


### Lowering notes

- Lowers `substr(string, offset, length?)` with target-local pointer arithmetic.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_substr_replace`

## Signature summary

```php
function substr(string $string, int $offset, int $length): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `substr()`](../../../php/builtins/string/substr.md)

