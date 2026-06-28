---
title: "vprintf() — internals"
description: "Compiler internals for vprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 388
---

## `vprintf()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:530](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L530) (`lower_vprintf`)
- **Function symbol**: `lower_vprintf()`


### Lowering notes

- Lowers `vprintf(format, values)` as `vsprintf()` followed by stdout emission.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function vprintf(string $format, array $values): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `vprintf()`](../../../php/builtins/string/vprintf.md)

