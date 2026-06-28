---
title: "ucfirst() — internals"
description: "Compiler internals for ucfirst(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 384
---

## `ucfirst()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:96](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L96) (`lower_ucfirst`)
- **Function symbol**: `lower_ucfirst()`


### Lowering notes

- Lowers `ucfirst()` by copying the string and uppercasing the first ASCII byte.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strcopy`

## Signature summary

```php
function ucfirst(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ucfirst()`](../../../php/builtins/string/ucfirst.md)

