---
title: "str_repeat() — internals"
description: "Compiler internals for str_repeat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 369
---

## `str_repeat()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:746](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L746) (`lower_str_repeat`)
- **Function symbol**: `lower_str_repeat()`


### Lowering notes

- Lowers `str_repeat(string, times)` through the shared runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_repeat`

## Signature summary

```php
function str_repeat(string $string, int $times): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `str_repeat()`](../../../php/builtins/string/str_repeat.md)

