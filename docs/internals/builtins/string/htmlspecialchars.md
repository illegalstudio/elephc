---
title: "htmlspecialchars() — internals"
description: "Compiler internals for htmlspecialchars(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 361
---

## `htmlspecialchars()` — internals

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
function htmlspecialchars(string $string, int $flags, string $encoding, bool $double_encode): string
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Cross-references

- [User reference for `htmlspecialchars()`](../../../php/builtins/string/htmlspecialchars.md)

