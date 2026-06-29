---
title: "strtoupper() — internals"
description: "Compiler internals for strtoupper(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 382
---

## `strtoupper()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:76](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L76) (`lower_unary_string_runtime`)
- **Function symbol**: `lower_unary_string_runtime()`


### Lowering notes

- Lowers a one-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_grapheme_strrev`
- `__rt_strcopy`

## Signature summary

```php
function strtoupper(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `strtoupper()`](../../../php/builtins/string/strtoupper.md)

