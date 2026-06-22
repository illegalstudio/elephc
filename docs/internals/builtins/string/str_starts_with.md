---
title: "str_starts_with() — internals"
description: "Compiler internals for str_starts_with(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 372
---

## `str_starts_with()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:139](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L139) (`lower_binary_string_runtime`)
- **Function symbol**: `lower_binary_string_runtime()`


### Lowering notes

- Lowers a two-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_explode`

## Signature summary

```php
function str_starts_with(string $haystack, string $needle): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `str_starts_with()`](../../../php/builtins/string/str_starts_with.md)

