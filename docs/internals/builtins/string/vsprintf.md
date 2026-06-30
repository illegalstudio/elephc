---
title: "vsprintf() — internals"
description: "Compiler internals for vsprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 406
---

## `vsprintf()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:524](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L524) (`lower_vsprintf`)
- **Function symbol**: `lower_vsprintf()`


### Lowering notes

- Lowers `vsprintf(format, values)` through the array-to-sprintf runtime bridge.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function vsprintf(string $format, array $values): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `vsprintf()`](../../../php/builtins/string/vsprintf.md)

