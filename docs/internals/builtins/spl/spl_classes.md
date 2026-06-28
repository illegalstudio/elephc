---
title: "spl_classes() — internals"
description: "Compiler internals for spl_classes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 311
---

## `spl_classes()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:205](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L205) (`lower_spl_classes`)
- **Function symbol**: `lower_spl_classes()`


### Lowering notes

- Lowers `spl_classes()` to the static compiler-shipped SPL/core type snapshot.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_itoa`

## Signature summary

```php
function spl_classes(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `spl_classes()`](../../../php/builtins/spl/spl_classes.md)

