---
title: "sprintf() — internals"
description: "Compiler internals for sprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 361
---

## `sprintf()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:511](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L511) (`lower_sprintf`)
- **Function symbol**: `lower_sprintf()`


### Lowering notes

- Lowers `sprintf(format, values...)` by packing variadic records for `__rt_sprintf`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function sprintf(string $format, ...$values): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `sprintf()`](../../../php/builtins/string/sprintf.md)

