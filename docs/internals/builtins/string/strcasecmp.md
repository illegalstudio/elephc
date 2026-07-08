---
title: "strcasecmp() — internals"
description: "Compiler internals for strcasecmp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 389
---

## `strcasecmp()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/strcasecmp.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strcasecmp.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:139](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L139) (`lower_binary_string_runtime`)
- **Function symbol**: `lower_binary_string_runtime()`


### Lowering notes

- Lowers a two-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_explode`

## Signature summary

```php
function strcasecmp(string $string1, string $string2): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `strcasecmp()`](../../../php/builtins/string/strcasecmp.md)
