---
title: "str_pad() — internals"
description: "Compiler internals for str_pad(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 368
---

## `str_pad()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:818](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L818) (`lower_str_pad`)
- **Function symbol**: `lower_str_pad()`


### Lowering notes

- Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_pad`

## Signature summary

```php
function str_pad(string $string, int $length, string $pad_string, int $pad_type): string
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `str_pad()`](../../../php/builtins/string/str_pad.md)

