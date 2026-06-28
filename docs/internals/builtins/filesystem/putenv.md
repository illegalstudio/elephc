---
title: "putenv() — internals"
description: "Compiler internals for putenv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 119
---

## `putenv()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:657](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L657) (`lower_putenv`)
- **Function symbol**: `lower_putenv()`


### Lowering notes

- Lowers `putenv(assignment)` by copying the environment string into persistent heap storage.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_php_uname`

## Signature summary

```php
function putenv(string $assignment): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `putenv()`](../../../php/builtins/filesystem/putenv.md)

