---
title: "nl2br() — internals"
description: "Compiler internals for nl2br(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 353
---

## `nl2br()` — internals

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
function nl2br(string $string, bool $use_xhtml): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `nl2br()`](../../../php/builtins/string/nl2br.md)

