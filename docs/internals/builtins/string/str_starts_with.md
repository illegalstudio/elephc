---
title: "str_starts_with() — internals"
description: "Compiler internals for str_starts_with(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 394
---

## `str_starts_with()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/str_starts_with.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_starts_with.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:157](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L157) (`lower_binary_string_runtime`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/str_starts_with.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_starts_with.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `str_starts_with()`](../../../php/builtins/string/str_starts_with.md)
