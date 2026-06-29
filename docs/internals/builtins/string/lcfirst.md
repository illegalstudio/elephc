---
title: "lcfirst() — internals"
description: "Compiler internals for lcfirst(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 351
---

## `lcfirst()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:104](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L104) (`lower_lcfirst`)
- **Function symbol**: `lower_lcfirst()`


### Lowering notes

- Lowers `lcfirst()` by copying the string and lowercasing the first ASCII byte.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strcopy`

## Signature summary

```php
function lcfirst(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `lcfirst()`](../../../php/builtins/string/lcfirst.md)

